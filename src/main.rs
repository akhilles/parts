use clap::{Parser, Subcommand};
use tracing::{error, info};

mod digikey;
mod parts;

#[derive(Parser)]
#[command(name = "parts")]
#[command(about = "A PCB part library CLI tool")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate parts from various sources
    Gen {
        #[command(subcommand)]
        source: GenCommands,
    },
    /// Fetch information about specific parts
    Fetch {
        #[command(subcommand)]
        source: FetchCommands,
    },
}

#[derive(Subcommand)]
enum GenCommands {
    /// Generate parts from Digikey manufacturers
    DigikeyManufacturers {
        /// Force refresh manufacturers even if they are fresh
        #[arg(long)]
        force: bool,
    },
    /// Show information about parts in the parts list
    DigikeyPartInfo {
        /// Force refresh all parts even if they are fresh
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum FetchCommands {
    /// Fetch part information from Digikey
    DigikeyPartInfo {
        /// The manufacturer part number (MPN) to look up
        mpn: String,
    },
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("parts=debug".parse().unwrap()))
        .init();

    info!("Parts CLI starting");

    let cli = Cli::parse();

    match cli.command {
        Commands::Gen { source } => {
            match source {
                GenCommands::DigikeyManufacturers { force } => {
                    info!("Updating Digikey manufacturers (force: {})", force);
                    
                    match digikey::DigikeyClient::update_manufacturers_if_stale(force).await {
                        Ok(()) => {
                            println!("✅ Successfully updated manufacturers information");
                        },
                        Err(e) => {
                            eprintln!("❌ Failed to update manufacturers: {:?}", e);
                            match e {
                                digikey::DigikeyError::EnvVar(_) => {
                                    eprintln!("Make sure DIGIKEY_CLIENT_ID and DIGIKEY_CLIENT_SECRET environment variables are set.");
                                    eprintln!("You can get these from your Digikey developer account at https://developer.digikey.com");
                                },
                                digikey::DigikeyError::Api(api_err) => {
                                    eprintln!("API Error: {}", api_err);
                                    eprintln!("Check if your credentials are valid and you have access to the Product Information API.");
                                },
                                _ => {}
                            }
                        }
                    }
                },
                GenCommands::DigikeyPartInfo { force } => {
                    info!("Updating Digikey part information (force: {})", force);
                    
                    match digikey::DigikeyClient::update_stale_parts(force).await {
                        Ok(()) => {
                            println!("✅ Successfully updated parts information");
                        },
                        Err(e) => {
                            eprintln!("❌ Failed to update parts: {:?}", e);
                        }
                    }
                }
            }
        },
        Commands::Fetch { source } => {
            match source {
                FetchCommands::DigikeyPartInfo { mpn } => {
                    info!("Fetching Digikey part information for: {}", mpn);
                    
                    match digikey::DigikeyClient::new() {
                        Ok(client) => {
                            match client.get_part_details(&mpn).await {
                                Ok(details) => {
                                    println!("Manufacturer: {}", details.manufacturer);
                                    println!("DigiKey Product Numbers: {:?}", details.digikey_product_numbers);
                                    println!("Description: {}", details.detailed_description);
                                    println!("Category: {}", details.category);
                                    
                                    if let Some(datasheet) = &details.datasheet_url {
                                        println!("Datasheet: {}", datasheet);
                                    } else {
                                        println!("Datasheet: Not available");
                                    }
                                    
                                    if let Some(product_url) = &details.product_url {
                                        println!("Product URL: {}", product_url);
                                    }
                                    
                                    println!("Product Status: {}", details.product_status.as_str());
                                    println!("Quantity Available: {}", details.quantity_available);
                                    
                                    if let Some(price) = details.unit_price {
                                        println!("Unit Price: ${:.2}", price);
                                    }
                                    
                                    if let Some(photo) = &details.photo_url {
                                        println!("Photo URL: {}", photo);
                                    }
                                    
                                    println!("Discontinued: {}", details.discontinued);
                                    println!("End of Life: {}", details.end_of_life);
                                    println!("Normally Stocking: {}", details.normally_stocking);
                                },
                                Err(e) => {
                                    error!("Failed to fetch part details: {:?}", e);
                                    eprintln!("Failed to fetch part details: {:?}", e);
                                    match e {
                                        digikey::DigikeyError::EnvVar(_) => {
                                            eprintln!("Make sure DIGIKEY_CLIENT_ID and DIGIKEY_CLIENT_SECRET environment variables are set.");
                                        },
                                        digikey::DigikeyError::Api(api_err) => {
                                            eprintln!("API Error: {}", api_err);
                                            eprintln!("Check if the part number '{}' exists in Digikey's database.", mpn);
                                        },
                                        _ => {}
                                    }
                                }
                            }
                        },
                        Err(e) => {
                            error!("Failed to create Digikey client: {:?}", e);
                            eprintln!("Failed to create Digikey client: {:?}", e);
                        }
                    }
                }
            }
        }
    }
}
