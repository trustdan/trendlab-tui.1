//! Yahoo Finance data provider.
//!
//! Fetches daily OHLCV bars from Yahoo's v8 chart API. Handles rate limiting,
//! retries with exponential backoff, response parsing, and the circuit breaker.
//!
//! Yahoo Finance has no official API and is subject to unannounced format changes.
//! The CSV import path is the primary fallback when Yahoo is unavailable.

use super::circuit_breaker::CircuitBreaker;
use super::provider::{DataError, DataProvider, DataSource, FetchResult, RawBar};
use chrono::NaiveDate;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

/// Yahoo Finance v8 chart API response.
#[derive(Debug, Deserialize)]
struct ChartResponse {
    chart: ChartResult,
}

#[derive(Debug, Deserialize)]
struct ChartResult {
    result: Option<Vec<ChartData>>,
    error: Option<ChartError>,
}

#[derive(Debug, Deserialize)]
struct ChartError {
    code: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct ChartData {
    timestamp: Option<Vec<i64>>,
    indicators: Indicators,
}

#[derive(Debug, Deserialize)]
struct Indicators {
    quote: Vec<QuoteData>,
    adjclose: Option<Vec<AdjCloseData>>,
}

#[derive(Debug, Deserialize)]
struct QuoteData {
    open: Vec<Option<f64>>,
    high: Vec<Option<f64>>,
    low: Vec<Option<f64>>,
    close: Vec<Option<f64>>,
    volume: Vec<Option<u64>>,
}

#[derive(Debug, Deserialize)]
struct AdjCloseData {
    adjclose: Vec<Option<f64>>,
}

/// Yahoo Finance data provider.
pub struct YahooProvider {
    client: reqwest::blocking::Client,
    circuit_breaker: Arc<CircuitBreaker>,
    max_retries: u32,
    base_delay: Duration,
}

impl YahooProvider {
    pub fn new(circuit_breaker: Arc<CircuitBreaker>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            circuit_breaker,
            max_retries: 3,
            base_delay: Duration::from_millis(500),
        }
    }

    /// Build the chart API URL for a symbol and date range.
    fn chart_url(symbol: &str, start: NaiveDate, end: NaiveDate) -> String {
        let start_ts = start.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        let end_ts = end.and_hms_opt(23, 59, 59).unwrap().and_utc().timestamp();
        format!(
            "https://query2.finance.yahoo.com/v8/finance/chart/{symbol}\
             ?period1={start_ts}&period2={end_ts}&interval=1d\
             &includeAdjustedClose=true"
        )
    }

    /// Parse the chart API response into RawBars.
    fn parse_response(symbol: &str, resp: ChartResponse) -> Result<Vec<RawBar>, DataError> {
        let result = resp.chart.result.ok_or_else(|| {
            if let Some(err) = resp.chart.error {
                if err.code == "Not Found" {
                    DataError::SymbolNotFound {
                        symbol: symbol.to_string(),
                    }
                } else {
                    DataError::ResponseFormatChanged(format!("{}: {}", err.code, err.description))
                }
            } else {
                DataError::ResponseFormatChanged("empty result with no error".into())
            }
        })?;

        let data = result
            .into_iter()
            .next()
            .ok_or_else(|| DataError::ResponseFormatChanged("result array is empty".into()))?;

        let timestamps = data
            .timestamp
            .ok_or_else(|| DataError::ResponseFormatChanged("no timestamps".into()))?;

        let quote = data
            .indicators
            .quote
            .into_iter()
            .next()
            .ok_or_else(|| DataError::ResponseFormatChanged("no quote data".into()))?;

        let adj_closes = data
            .indicators
            .adjclose
            .and_then(|v| v.into_iter().next())
            .map(|a| a.adjclose);

        let n = timestamps.len();
        let mut bars = Vec::with_capacity(n);

        for (i, &ts) in timestamps.iter().enumerate() {
            let date = chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.naive_utc().date())
                .ok_or_else(|| {
                    DataError::ResponseFormatChanged(format!("invalid timestamp: {ts}"))
                })?;

            let open = quote.open.get(i).copied().flatten();
            let high = quote.high.get(i).copied().flatten();
            let low = quote.low.get(i).copied().flatten();
            let close = quote.close.get(i).copied().flatten();
            let volume = quote.volume.get(i).copied().flatten();
            let adj_close = adj_closes
                .as_ref()
                .and_then(|v| v.get(i).copied().flatten());

            // Skip bars where all OHLCV are None (holidays/non-trading days)
            if open.is_none()
                && high.is_none()
                && low.is_none()
                && close.is_none()
                && volume.is_none()
            {
                continue;
            }

            bars.push(RawBar {
                date,
                open: open.unwrap_or(f64::NAN),
                high: high.unwrap_or(f64::NAN),
                low: low.unwrap_or(f64::NAN),
                close: close.unwrap_or(f64::NAN),
                volume: volume.unwrap_or(0),
                adj_close: adj_close.unwrap_or(f64::NAN),
            });
        }

        if bars.is_empty() {
            return Err(DataError::SymbolNotFound {
                symbol: symbol.to_string(),
            });
        }

        Ok(bars)
    }

    /// Execute a single HTTP request with retry and circuit breaker logic.
    fn fetch_with_retry(
        &self,
        symbol: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<RawBar>, DataError> {
        if !self.circuit_breaker.is_allowed() {
            return Err(DataError::CircuitBreakerTripped);
        }

        let url = Self::chart_url(symbol, start, end);
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let delay = self.base_delay * 2u32.pow(attempt - 1);
                std::thread::sleep(delay);
            }

            if !self.circuit_breaker.is_allowed() {
                return Err(DataError::CircuitBreakerTripped);
            }

            match self.client.get(&url).send() {
                Ok(resp) => {
                    let status = resp.status();

                    if status == reqwest::StatusCode::FORBIDDEN {
                        // IP ban — immediately trip the circuit breaker
                        self.circuit_breaker.trip();
                        return Err(DataError::CircuitBreakerTripped);
                    }

                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                        self.circuit_breaker.record_failure();
                        let retry_after = resp
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or(60);
                        last_error = Some(DataError::RateLimited {
                            retry_after_secs: retry_after,
                        });
                        continue;
                    }

                    if status == reqwest::StatusCode::UNAUTHORIZED {
                        return Err(DataError::AuthenticationRequired(
                            "Yahoo Finance requires authentication".into(),
                        ));
                    }

                    if !status.is_success() {
                        self.circuit_breaker.record_failure();
                        last_error = Some(DataError::Other(format!("HTTP {status} for {symbol}")));
                        continue;
                    }

                    // Parse the JSON response
                    let chart: ChartResponse = resp.json().map_err(|e| {
                        DataError::ResponseFormatChanged(format!(
                            "failed to parse response for {symbol}: {e}"
                        ))
                    })?;

                    let bars = Self::parse_response(symbol, chart)?;
                    self.circuit_breaker.record_success();
                    return Ok(bars);
                }
                Err(e) => {
                    if e.is_connect() || e.is_timeout() {
                        last_error = Some(DataError::NetworkUnreachable(e.to_string()));
                        continue;
                    }
                    return Err(DataError::NetworkUnreachable(e.to_string()));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| DataError::Other("max retries exceeded".into())))
    }
}

impl DataProvider for YahooProvider {
    fn name(&self) -> &str {
        "yahoo_finance"
    }

    fn fetch(
        &self,
        symbol: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<FetchResult, DataError> {
        let bars = self.fetch_with_retry(symbol, start, end)?;
        Ok(FetchResult {
            symbol: symbol.to_string(),
            bars,
            source: DataSource::YahooFinance,
        })
    }

    fn is_available(&self) -> bool {
        self.circuit_breaker.is_allowed()
    }
}
