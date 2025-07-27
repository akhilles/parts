#![allow(dead_code)]

use crate::api_cache::{ApiCache, CacheRequest};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};

const DIGIKEY_TOKEN_URL: &str = "https://api.digikey.com/v1/oauth2/token";
const DIGIKEY_API_BASE: &str = "https://api.digikey.com/products/v4";
const TOKEN_CACHE_FILENAME: &str = "digikey_token.json";
const MAX_RETRIES: u32 = 3;

#[derive(Debug)]
pub struct DigikeyClient {
    client: Client,
    client_id: String,
    client_secret: String,
    token_cache_path: PathBuf,
    api_cache: ApiCache,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    #[serde(default)]
    pub created_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManufacturersResponse {
    #[serde(rename = "Manufacturers")]
    pub manufacturers: Vec<Manufacturer>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Manufacturer {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "HomePage")]
    pub home_page: Option<String>,
}

// Product Details API Structs
#[derive(Debug, Serialize, Deserialize)]
pub struct ProductDetailsResponse {
    #[serde(rename = "SearchLocaleUsed")]
    pub search_locale_used: Option<SearchLocale>,
    #[serde(rename = "Product")]
    pub product: Product,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchLocale {
    #[serde(rename = "Site")]
    pub site: Option<String>,
    #[serde(rename = "Language")]
    pub language: Option<String>,
    #[serde(rename = "Currency")]
    pub currency: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Product {
    #[serde(rename = "ManufacturerProductNumber")]
    pub manufacturer_product_number: String,
    #[serde(rename = "Manufacturer")]
    pub manufacturer: ManufacturerInfo,
    #[serde(rename = "Description")]
    pub description: Description,
    #[serde(rename = "DatasheetUrl")]
    pub datasheet_url: Option<String>,
    #[serde(rename = "ProductUrl")]
    pub product_url: Option<String>,
    #[serde(rename = "Category")]
    pub category: Option<CategoryNode>,
    #[serde(rename = "ProductVariations")]
    pub product_variations: Option<Vec<ProductVariation>>,
    #[serde(rename = "QuantityAvailable")]
    pub quantity_available: Option<i32>,
    #[serde(rename = "ProductStatus")]
    pub product_status: Option<ProductStatus>,
    #[serde(rename = "StandardPricing")]
    pub standard_pricing: Option<Vec<PriceBreak>>,
    #[serde(rename = "UnitPrice")]
    pub unit_price: Option<f64>,
    #[serde(rename = "PhotoUrl")]
    pub photo_url: Option<String>,
    #[serde(rename = "Discontinued")]
    pub discontinued: Option<bool>,
    #[serde(rename = "EndOfLife")]
    pub end_of_life: Option<bool>,
    #[serde(rename = "NormallyStocking")]
    pub normally_stocking: Option<bool>,
    // Include all other fields as Option<serde_json::Value> for investigation
    #[serde(flatten)]
    pub extra_fields: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManufacturerInfo {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "Name")]
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Description {
    #[serde(rename = "DetailedDescription")]
    pub detailed_description: Option<String>,
    #[serde(rename = "CatalogDescription")]
    pub catalog_description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryNode {
    #[serde(rename = "Id")]
    pub id: Option<i32>,
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(rename = "Parent")]
    pub parent: Option<Box<CategoryNode>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProductVariation {
    #[serde(rename = "DigiKeyProductNumber")]
    pub digikey_product_number: Option<String>,
    #[serde(rename = "PackageType")]
    pub package_type: Option<PackageType>,
    #[serde(rename = "StandardPricing")]
    pub standard_pricing: Option<Vec<PriceBreak>>,
    #[serde(rename = "QuantityAvailableforPackageType")]
    pub quantity_available_for_package_type: Option<i32>,
    #[serde(flatten)]
    pub extra_fields: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProductStatus {
    #[serde(rename = "Id")]
    pub id: Option<i32>,
    #[serde(rename = "Status")]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageType {
    #[serde(rename = "Id")]
    pub id: Option<i32>,
    #[serde(rename = "Name")]
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PriceBreak {
    #[serde(rename = "BreakQuantity")]
    pub break_quantity: Option<i32>,
    #[serde(rename = "UnitPrice")]
    pub unit_price: Option<f64>,
    #[serde(rename = "TotalPrice")]
    pub total_price: Option<f64>,
}

#[derive(Debug, Clone)]
pub enum ProductStatusEnum {
    Active,
    Discontinued,
    Obsolete,
    EndOfLife,
    NotForNewDesigns,
    Preliminary,
    Other(String),
}

impl ProductStatusEnum {
    pub fn from_string(status: &str) -> Self {
        match status {
            "Active" => ProductStatusEnum::Active,
            "Discontinued" => ProductStatusEnum::Discontinued,
            "Obsolete" => ProductStatusEnum::Obsolete,
            "End of Life" => ProductStatusEnum::EndOfLife,
            "Not For New Designs" => ProductStatusEnum::NotForNewDesigns,
            "Preliminary" => ProductStatusEnum::Preliminary,
            other => ProductStatusEnum::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            ProductStatusEnum::Active => "Active",
            ProductStatusEnum::Discontinued => "Discontinued",
            ProductStatusEnum::Obsolete => "Obsolete",
            ProductStatusEnum::EndOfLife => "End of Life",
            ProductStatusEnum::NotForNewDesigns => "Not For New Designs",
            ProductStatusEnum::Preliminary => "Preliminary",
            ProductStatusEnum::Other(s) => s,
        }
    }
}

// Simplified struct for easier consumption
#[derive(Debug, Clone)]
pub struct PartDetails {
    pub manufacturer: String,
    pub digikey_product_numbers: Vec<String>,
    pub detailed_description: String,
    pub datasheet_url: Option<String>,
    pub category: String,
    pub product_url: Option<String>,
    pub product_status: ProductStatusEnum,
    pub quantity_available: i32,
    pub unit_price: Option<f64>,
    pub photo_url: Option<String>,
    pub discontinued: bool,
    pub end_of_life: bool,
    pub normally_stocking: bool,
}

#[derive(Error, Debug)]
pub enum DigikeyError {
    #[error("Environment variable error: {0}")]
    EnvVar(#[from] env::VarError),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("KDL parsing error: {0}")]
    Kdl(#[from] kdl::KdlError),

    #[error("Chrono parsing error: {0}")]
    Chrono(#[from] chrono::ParseError),

    #[error("Cache error: {0}")]
    Cache(#[from] crate::api_cache::CacheError),

    #[error("Token has expired and needs refresh")]
    TokenExpired,

    #[error("API error: {0}")]
    Api(String),

    #[error("Other error: {0}")]
    Other(String),
}

impl DigikeyClient {
    pub fn new() -> Result<Self, DigikeyError> {
        debug!("Creating new Digikey client");

        let client_id = env::var("DIGIKEY_CLIENT_ID")?;
        let client_secret = env::var("DIGIKEY_CLIENT_SECRET")?;

        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("./cache"))
            .join("parts");

        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)?;
            debug!("Created cache directory: {:?}", cache_dir);
        }

        let token_cache_path = cache_dir.join(TOKEN_CACHE_FILENAME);
        let api_cache_dir = cache_dir.join("digikey-api");
        let api_cache = ApiCache::new(api_cache_dir, 24 * 60 * 60); // 24 hour default TTL

        Ok(Self {
            client: Client::new(),
            client_id,
            client_secret,
            token_cache_path,
            api_cache,
        })
    }

    fn load_cached_token(&self) -> Option<TokenResponse> {
        debug!(
            "Attempting to load cached token from {:?}",
            self.token_cache_path
        );

        match fs::read_to_string(&self.token_cache_path) {
            Ok(content) => {
                debug!("Found cached token file, parsing JSON");
                match serde_json::from_str::<TokenResponse>(&content) {
                    Ok(token) => {
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        let expires_at = token.created_at + token.expires_in;

                        if now < expires_at - 60 {
                            // Leave 60 second buffer
                            debug!(
                                "Loaded valid cached token, expires in {} seconds",
                                expires_at - now
                            );
                            Some(token)
                        } else {
                            warn!(
                                "Cached token has expired (expires_at: {}, now: {})",
                                expires_at, now
                            );
                            None
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse cached token: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                debug!("No cached token found: {}", e);
                None
            }
        }
    }

    fn save_token_to_cache(&self, token: &TokenResponse) -> Result<(), DigikeyError> {
        debug!("Saving token to cache at {:?}", self.token_cache_path);

        let mut token_with_timestamp = token.clone();
        token_with_timestamp.created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let json = serde_json::to_string_pretty(&token_with_timestamp)?;
        fs::write(&self.token_cache_path, json)?;

        info!("Token saved to cache successfully");
        Ok(())
    }

    async fn get_client_credentials_token(&self) -> Result<TokenResponse, DigikeyError> {
        info!("Requesting access token using client credentials flow");

        debug!(
            "Client ID: {}...",
            &self.client_id[..8.min(self.client_id.len())]
        );
        debug!(
            "Client Secret: {}...",
            &self.client_secret[..8.min(self.client_secret.len())]
        );

        debug!("Sending token request to Digikey at {}", DIGIKEY_TOKEN_URL);

        let form_body = format!(
            "client_id={}&client_secret={}&grant_type=client_credentials",
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&self.client_secret)
        );

        let response = self
            .client
            .post(DIGIKEY_TOKEN_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_body)
            .send()
            .await?;

        let status = response.status();
        debug!("Token request response status: {}", status);

        if !status.is_success() {
            let error_text = response.text().await?;
            error!(
                "Token request failed with status {}: {}",
                status, error_text
            );
            return Err(DigikeyError::Api(format!(
                "Token request failed: {error_text}"
            )));
        }

        let token_response: TokenResponse = response.json().await?;
        info!(
            "Token request successful, expires in {} seconds",
            token_response.expires_in
        );

        self.save_token_to_cache(&token_response)?;

        Ok(token_response)
    }

    async fn get_valid_token(&self) -> Result<String, DigikeyError> {
        debug!("Getting valid access token");

        // Try cached token first
        if let Some(token) = self.load_cached_token() {
            debug!("Using cached token");
            return Ok(token.access_token);
        }

        // Get new token using client credentials
        info!("No valid cached token, requesting new token");
        let token_response = self.get_client_credentials_token().await?;
        Ok(token_response.access_token)
    }

    pub async fn get_manufacturers(&self) -> Result<ManufacturersResponse, DigikeyError> {
        info!("Fetching manufacturers from Digikey API");

        let token = self.get_valid_token().await?;
        let url = format!("{DIGIKEY_API_BASE}/search/manufacturers");

        debug!("Making request to: {}", url);

        let mut retry_count = 0;

        loop {
            if retry_count > 0 {
                debug!("API request attempt #{}", retry_count + 1);
            }

            let response = self
                .client
                .get(&url)
                .header("Authorization", format!("Bearer {token}"))
                .header("X-DIGIKEY-Client-Id", &self.client_id)
                .header("X-DIGIKEY-Locale-Language", "en")
                .header("X-DIGIKEY-Locale-Currency", "USD")
                .header("X-DIGIKEY-Locale-Site", "US")
                .send()
                .await?;

            let status = response.status();
            debug!("API response status: {}", status);

            if status.is_success() {
                info!("Parsing manufacturers JSON response");
                let parse_start = Instant::now();
                let manufacturers: ManufacturersResponse = response.json().await?;
                let parse_duration = parse_start.elapsed();
                info!(
                    "Successfully fetched {} manufacturers (parsed in {:?})",
                    manufacturers.manufacturers.len(),
                    parse_duration
                );
                return Ok(manufacturers);
            } else if status == 429 {
                // Rate limit handling
                warn!("Rate limited by Digikey API (429)");

                if let Some(retry_after) = response.headers().get("Retry-After") {
                    if let Ok(retry_after_str) = retry_after.to_str() {
                        if let Ok(seconds) = retry_after_str.parse::<u64>() {
                            warn!(
                                "Rate limited, retrying after {} seconds (from Retry-After header)",
                                seconds
                            );
                            sleep(Duration::from_secs(seconds)).await;
                            continue;
                        }
                    }
                }

                retry_count += 1;
                if retry_count >= MAX_RETRIES {
                    error!("Max retries exceeded for rate limiting");
                    break;
                }

                let delay = 2_u64.pow(retry_count) * 60; // Exponential backoff
                warn!(
                    "Rate limited, retrying in {} seconds (attempt {} of {})",
                    delay,
                    retry_count + 1,
                    MAX_RETRIES
                );
                sleep(Duration::from_secs(delay)).await;
            } else if status == 401 {
                // Token might be expired, try getting a new one
                warn!("Received 401 Unauthorized, token may be expired. Getting new token.");
                let new_token = self.get_client_credentials_token().await?;

                // Retry the request with new token
                debug!("Retrying request with new token");
                let response = self
                    .client
                    .get(&url)
                    .header(
                        "Authorization",
                        format!("Bearer {}", new_token.access_token),
                    )
                    .header("X-DIGIKEY-Client-Id", &self.client_id)
                    .header("X-DIGIKEY-Locale-Language", "en")
                    .header("X-DIGIKEY-Locale-Currency", "USD")
                    .header("X-DIGIKEY-Locale-Site", "US")
                    .send()
                    .await?;

                let retry_status = response.status();
                if retry_status.is_success() {
                    let manufacturers: ManufacturersResponse = response.json().await?;
                    info!(
                        "Successfully fetched {} manufacturers with new token",
                        manufacturers.manufacturers.len()
                    );
                    return Ok(manufacturers);
                } else {
                    let error_text = response.text().await?;
                    error!(
                        "API request failed even with new token: {} - {}",
                        retry_status, error_text
                    );
                    return Err(DigikeyError::Api(format!(
                        "API request failed: {retry_status} - {error_text}"
                    )));
                }
            } else {
                let error_text = response.text().await?;
                error!("API request failed with status {}: {}", status, error_text);
                return Err(DigikeyError::Api(format!(
                    "API request failed: {status} - {error_text}"
                )));
            }
        }

        Err(DigikeyError::Other(
            "Max retries exceeded for rate limiting".to_string(),
        ))
    }

    /// Make a cached API request to Digikey
    async fn cached_api_request(
        &self,
        method: &str,
        path: &str,
        path_params: &HashMap<String, String>,
        query_params: &HashMap<String, String>,
        force_refresh: bool,
    ) -> Result<String, DigikeyError> {
        // Try cache first
        match self.api_cache.get(
            method,
            DIGIKEY_API_BASE,
            path,
            path_params,
            query_params,
            force_refresh,
        ) {
            Ok(cached_response) => {
                debug!("Using cached response for {} {}", method, path);
                return Ok(cached_response);
            }
            Err(_) => {
                debug!("Cache miss for {} {}, making API request", method, path);
            }
        }

        // Build the full URL
        let mut url = format!("{DIGIKEY_API_BASE}{path}");
        for (key, value) in path_params {
            let placeholder = format!("{{{key}}}");
            url = url.replace(&placeholder, &urlencoding::encode(value));
        }

        if !query_params.is_empty() {
            let query_string = query_params
                .iter()
                .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
                .collect::<Vec<_>>()
                .join("&");
            url = format!("{url}?{query_string}");
        }

        // Get token and make request with retry logic
        let token = self.get_valid_token().await?;
        debug!("Making cached request to: {}", url);

        let mut retry_count = 0;
        let (_status, response_text) = loop {
            let response = self
                .client
                .request(
                    reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
                    &url,
                )
                .header("Authorization", format!("Bearer {token}"))
                .header("X-DIGIKEY-Client-Id", &self.client_id)
                .header("X-DIGIKEY-Locale-Language", "en")
                .header("X-DIGIKEY-Locale-Currency", "USD")
                .header("X-DIGIKEY-Locale-Site", "US")
                .send()
                .await?;

            let status = response.status();

            if status.is_success() {
                let response_text = response.text().await?;
                break (status, response_text);
            } else if status == 401 {
                // Unauthorized - refresh token and retry
                warn!("401 Unauthorized - refreshing token and retrying");
                let _response_text = response.text().await?; // Consume response
                
                retry_count += 1;
                if retry_count >= MAX_RETRIES {
                    error!("Max retries exceeded for token refresh");
                    return Err(DigikeyError::Api(format!("HTTP {status}: Unauthorized")));
                }
                
                // Get a fresh token
                let new_token = self.get_client_credentials_token().await?;
                self.save_token_to_cache(&new_token)?;
                
                warn!("Token refreshed, retrying request (attempt {}/{})", retry_count + 1, MAX_RETRIES);
                continue;
            } else if status == 429 {
                // Rate limit handling - get headers before consuming response
                warn!("Rate limited by Digikey API (429)");

                let retry_after_header = response.headers().get("Retry-After")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());

                let _response_text = response.text().await?; // Consume response

                retry_count += 1;
                if retry_count >= MAX_RETRIES {
                    error!("Max retries exceeded for rate limiting");
                    return Err(DigikeyError::Api(format!("HTTP {status}: Rate limited")));
                }

                if let Some(seconds) = retry_after_header {
                    warn!(
                        "Rate limited, retrying after {} seconds from Retry-After header (attempt {}/{})",
                        seconds, retry_count + 1, MAX_RETRIES
                    );
                    sleep(Duration::from_secs(seconds)).await;
                } else {
                    let delay = Duration::from_secs(2_u64.pow(retry_count) * 60);
                    warn!(
                        "Rate limited, retrying in {:?} (attempt {}/{})",
                        delay,
                        retry_count + 1,
                        MAX_RETRIES
                    );
                    sleep(delay).await;
                }
            } else if status.is_server_error() {
                // 5xx server errors
                let _response_text = response.text().await?; // Consume response

                retry_count += 1;
                if retry_count >= MAX_RETRIES {
                    error!("Max retries exceeded for server errors");
                    return Err(DigikeyError::Api(format!("HTTP {status}: Server error")));
                }

                let delay = Duration::from_secs(2_u64.pow(retry_count) * 60);
                warn!(
                    "Server error {}, retrying in {:?} (attempt {}/{})",
                    status,
                    delay,
                    retry_count + 1,
                    MAX_RETRIES
                );
                sleep(delay).await;
            } else {
                // Client errors (4xx except 429) are not retryable
                let response_text = response.text().await?;
                return Err(DigikeyError::Api(format!("HTTP {status}: {response_text}")));
            }
        };

        // Cache the response
        let cache_request = CacheRequest {
            method,
            base_url: DIGIKEY_API_BASE,
            path,
            path_params,
            query_params,
        };
        if let Err(e) = self.api_cache.put(&cache_request, &response_text, None) {
            warn!("Failed to cache response: {:?}", e);
        }

        Ok(response_text)
    }

    /// Get manufacturers using cache
    pub async fn get_manufacturers_cached(
        &self,
        force_refresh: bool,
    ) -> Result<ManufacturersResponse, DigikeyError> {
        let response_text = self
            .cached_api_request(
                "GET",
                "/search/manufacturers",
                &HashMap::new(),
                &HashMap::new(),
                force_refresh,
            )
            .await?;

        let manufacturers: ManufacturersResponse = serde_json::from_str(&response_text)?;
        info!(
            "Fetched {} manufacturers (from cache or API)",
            manufacturers.manufacturers.len()
        );
        Ok(manufacturers)
    }

    /// Get part details using cache
    pub async fn get_part_details_cached(
        &self,
        product_number: &str,
        force_refresh: bool,
    ) -> Result<PartDetails, DigikeyError> {
        let mut path_params = HashMap::new();
        path_params.insert("productNumber".to_string(), product_number.to_string());

        let response_text = self
            .cached_api_request(
                "GET",
                "/search/{productNumber}/productdetails",
                &path_params,
                &HashMap::new(),
                force_refresh,
            )
            .await?;

        // Parse the JSON response
        let product: ProductDetailsResponse = serde_json::from_str(&response_text)?;

        // Convert to our PartDetails struct using the existing logic
        let manufacturer = product.product.manufacturer.name.clone();

        let digikey_product_numbers = if let Some(variations) = &product.product.product_variations
        {
            variations
                .iter()
                .filter_map(|v| v.digikey_product_number.as_ref())
                .cloned()
                .collect()
        } else {
            Vec::new()
        };

        let detailed_description = product
            .product
            .description
            .detailed_description
            .clone()
            .unwrap_or_else(|| "No description available".to_string());

        let category = product
            .product
            .category
            .as_ref()
            .and_then(|c| c.name.as_ref())
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string());

        let product_status = product
            .product
            .product_status
            .as_ref()
            .and_then(|s| s.status.as_ref())
            .map(|s| ProductStatusEnum::from_string(s))
            .unwrap_or(ProductStatusEnum::Other("Unknown".to_string()));

        Ok(PartDetails {
            manufacturer,
            digikey_product_numbers,
            detailed_description,
            datasheet_url: product.product.datasheet_url.clone(),
            category,
            product_url: product.product.product_url.clone(),
            product_status,
            quantity_available: product.product.quantity_available.unwrap_or(0),
            unit_price: product.product.unit_price,
            photo_url: product.product.photo_url.clone(),
            discontinued: product.product.discontinued.unwrap_or(false),
            end_of_life: product.product.end_of_life.unwrap_or(false),
            normally_stocking: product.product.normally_stocking.unwrap_or(false),
        })
    }





    pub async fn get_part_details(
        &self,
        product_number: &str,
    ) -> Result<PartDetails, DigikeyError> {
        info!("Fetching product details for: {}", product_number);

        let token = self.get_valid_token().await?;
        let url = format!(
            "{}/search/{}/productdetails",
            DIGIKEY_API_BASE,
            urlencoding::encode(product_number)
        );

        debug!("Making request to: {}", url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("X-DIGIKEY-Client-Id", &self.client_id)
            .header("X-DIGIKEY-Locale-Language", "en")
            .header("X-DIGIKEY-Locale-Currency", "USD")
            .header("X-DIGIKEY-Locale-Site", "US")
            .send()
            .await?;

        let status = response.status();
        let response_text = response.text().await?;

        debug!("API response status: {}", status);

        if !status.is_success() {
            return Err(DigikeyError::Api(format!("HTTP {status}: {response_text}")));
        }

        let product_details: ProductDetailsResponse = serde_json::from_str(&response_text)?;
        let product = &product_details.product;

        // Extract key information
        let manufacturer = product.manufacturer.name.clone();

        let digikey_product_numbers = if let Some(variations) = &product.product_variations {
            variations
                .iter()
                .filter_map(|v| v.digikey_product_number.as_ref())
                .cloned()
                .collect()
        } else {
            vec![]
        };

        let detailed_description = product
            .description
            .detailed_description
            .clone()
            .or_else(|| product.description.catalog_description.clone())
            .unwrap_or_else(|| "No description available".to_string());

        let category = product
            .category
            .as_ref()
            .and_then(|c| c.name.as_ref())
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string());

        let product_status = product
            .product_status
            .as_ref()
            .and_then(|s| s.status.as_ref())
            .map(|s| ProductStatusEnum::from_string(s))
            .unwrap_or(ProductStatusEnum::Other("Unknown".to_string()));

        Ok(PartDetails {
            manufacturer,
            digikey_product_numbers,
            detailed_description,
            datasheet_url: product.datasheet_url.clone(),
            category,
            product_url: product.product_url.clone(),
            product_status,
            quantity_available: product.quantity_available.unwrap_or(0),
            unit_price: product.unit_price,
            photo_url: product.photo_url.clone(),
            discontinued: product.discontinued.unwrap_or(false),
            end_of_life: product.end_of_life.unwrap_or(false),
            normally_stocking: product.normally_stocking.unwrap_or(false),
        })
    }

    pub async fn write_parts_as_json(
        &self,
        parts_with_details: &[(String, PartDetails)],
    ) -> Result<PathBuf, DigikeyError> {
        info!("Writing {} parts as individual JSON files", parts_with_details.len());

        // Create db/digikey/part_details directory if it doesn't exist
        let parts_dir = PathBuf::from("db").join("digikey").join("part_details");
        if !parts_dir.exists() {
            fs::create_dir_all(&parts_dir)?;
            info!("Created directory: {:?}", parts_dir);
        }

        for (mpn, details) in parts_with_details {
            // Create a clean part info struct without pricing/quantity
            let part_info = serde_json::json!({
                "mpn": mpn,
                "manufacturer": details.manufacturer,
                "digikey_part_numbers": details.digikey_product_numbers,
                "status": details.product_status.as_str(),
                "discontinued": details.discontinued,
                "end_of_life": details.end_of_life,
                "normally_stocking": details.normally_stocking,
                "category": details.category,
                "description": details.detailed_description,
                "urls": {
                    "product": details.product_url,
                    "datasheet": details.datasheet_url,
                    "image": details.photo_url
                }
            });

            // Use MPN as filename, sanitize for filesystem
            let safe_filename = mpn.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
            let file_path = parts_dir.join(format!("{}.json", safe_filename));
            
            let json_content = serde_json::to_string_pretty(&part_info)?;
            fs::write(&file_path, json_content)?;
            debug!("Wrote part info to: {:?}", file_path);
        }

        info!("Successfully wrote {} parts to individual JSON files in: {:?}", 
              parts_with_details.len(), parts_dir);

        Ok(parts_dir)
    }

    pub async fn update_parts(force_refresh: bool) -> Result<(), DigikeyError> {
        info!("Updating parts data (force_refresh: {})", force_refresh);

        // Get all MPNs from sources.kdl
        let all_mpns = match crate::parts::get_all_mpns().await {
            Some(mpns) => mpns,
            None => {
                return Err(DigikeyError::Other(
                    "Failed to read sources.kdl file".to_string(),
                ));
            }
        };

        if all_mpns.is_empty() {
            println!("No MPNs found in sources.kdl");
            return Ok(());
        }

        println!(
            "Fetching {} parts from Digikey API (with caching)",
            all_mpns.len()
        );

        // Fetch all parts - cache will handle freshness automatically
        let client = DigikeyClient::new()?;
        let mut all_parts_with_details = Vec::new();

        for mpn in &all_mpns {
            println!("  📦 Processing {mpn}");
            match client.get_part_details_cached(mpn, force_refresh).await {
                Ok(details) => {
                    all_parts_with_details.push((mpn.clone(), details));
                }
                Err(e) => {
                    eprintln!("  ❌ Failed to fetch {mpn}: {e:?}");
                }
            }
        }

        if all_parts_with_details.is_empty() {
            return Err(DigikeyError::Other(
                "No parts details were available".to_string(),
            ));
        }

        println!(
            "Writing {} parts as individual JSON files to db/digikey/part_details/",
            all_parts_with_details.len()
        );

        match client.write_parts_as_json(&all_parts_with_details).await {
            Ok(path) => {
                println!("✅ Parts written to: {}", path.display());
                Ok(())
            }
            Err(e) => {
                eprintln!("❌ Failed to write parts file: {e:?}");
                Err(e)
            }
        }
    }

    pub async fn get_part_details_formatted(
        &self,
        product_number: &str,
        format: &str,
    ) -> Result<String, DigikeyError> {
        match format {
            "raw" => self.get_part_details_json_cached(product_number, false).await,
            "json" => {
                let raw_json = self.get_part_details_json_cached(product_number, false).await?;
                let filtered_json = self.filter_json(&raw_json)?;
                Ok(filtered_json)
            }
            "flat" => {
                let raw_json = self.get_part_details_json_cached(product_number, false).await?;
                let filtered_json = self.filter_json(&raw_json)?;
                let flattened = self.flatten_json(&filtered_json)?;
                Ok(flattened)
            }
            _ => Err(DigikeyError::Api(format!("Unknown format: {format}. Supported formats: raw, json, flat")))
        }
    }

    /// Get part details JSON using cache
    pub async fn get_part_details_json_cached(
        &self,
        product_number: &str,
        force_refresh: bool,
    ) -> Result<String, DigikeyError> {
        let mut path_params = HashMap::new();
        path_params.insert("productNumber".to_string(), product_number.to_string());

        let response_text = self
            .cached_api_request(
                "GET",
                "/search/{productNumber}/productdetails",
                &path_params,
                &HashMap::new(),
                force_refresh,
            )
            .await?;

        Ok(response_text)
    }

    fn filter_json(&self, json_str: &str) -> Result<String, DigikeyError> {
        let mut value: Value = serde_json::from_str(json_str)?;
        
        // Remove SearchLocaleUsed
        if let Some(obj) = value.as_object_mut() {
            obj.remove("SearchLocaleUsed");
            
            // Remove ProductVariations from Product
            if let Some(product) = obj.get_mut("Product").and_then(|p| p.as_object_mut()) {
                product.remove("ProductVariations");
            }
        }
        
        Ok(serde_json::to_string_pretty(&value)?)
    }

    fn flatten_json(&self, json_str: &str) -> Result<String, DigikeyError> {
        let value: Value = serde_json::from_str(json_str)?;
        let mut result = Vec::new();
        
        fn flatten_value(path: &str, value: &Value, result: &mut Vec<String>) {
            match value {
                Value::Object(map) => {
                    for (key, val) in map {
                        let new_path = if path.is_empty() {
                            key.clone()
                        } else {
                            format!("{}.{}", path, key)
                        };
                        flatten_value(&new_path, val, result);
                    }
                }
                Value::Array(arr) => {
                    for (index, val) in arr.iter().enumerate() {
                        let new_path = format!("{}.{}", path, index);
                        flatten_value(&new_path, val, result);
                    }
                }
                _ => {
                    let value_str = match value {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        Value::Null => "null".to_string(),
                        _ => value.to_string(),
                    };
                    result.push(format!("{}: {}", path, value_str));
                }
            }
        }
        
        flatten_value("", &value, &mut result);
        Ok(result.join("\n"))
    }

    #[deprecated(note = "Use get_part_details_json_cached instead for better performance")]
    pub async fn get_part_details_json(
        &self,
        product_number: &str,
    ) -> Result<String, DigikeyError> {
        // Delegate to cached version with force_refresh=true
        self.get_part_details_json_cached(product_number, true).await
    }
}

fn escape_kdl_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn escape_kdl_identifier(s: &str) -> String {
    // If the identifier contains spaces or special characters, quote it
    if s.chars()
        .any(|c| !c.is_alphanumeric() && c != '_' && c != '-')
    {
        format!("\"{}\"", escape_kdl_string(s))
    } else {
        s.to_string()
    }
}
