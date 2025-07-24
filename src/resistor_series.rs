use anyhow::{Context, Result, anyhow};
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use std::fs;
use std::str::FromStr;

#[derive(Debug)]
pub struct ResistorSeries {
    pub name: String,
    pub manufacturer: String,
    pub package: String,
    pub power: PowerSpec,
    pub tolerance: f64,
    pub tcr: TcrSpec,
    pub temp: TempRange,
    pub aec_q200: Option<u8>,
    pub standard_values: Vec<String>,
    pub ranges: Vec<ResistorRange>,
    pub mpn_template: String, // e.g., "ERJ2RKF{value|iec4}X"
}

#[derive(Debug)]
pub struct PowerSpec {
    pub value: Decimal,
    pub unit: String,
}

#[derive(Debug)]
pub struct TcrSpec {
    pub value: Decimal,
    pub unit: String,
}

#[derive(Debug)]
pub struct TempRange {
    pub min: f64,
    pub max: f64,
    pub unit: String,
}

#[derive(Debug)]
pub struct ResistorRange {
    pub min: Decimal,
    pub max: Decimal,
    pub unit: Option<String>,
}

pub async fn parse_and_print(file_path: &str) -> Result<()> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path))?;

    let document = content
        .parse::<kdl::KdlDocument>()
        .with_context(|| "Failed to parse KDL document")?;

    let mut series_list = Vec::new();

    // Look for resistor-series node
    if let Some(resistor_series_node) = document.get("resistor-series") {
        if let Some(children) = resistor_series_node.children() {
            for child in children.nodes() {
                let series_name = child.name().value();
                let series = parse_resistor_series(series_name, child)?;
                series_list.push(series);
            }
        }
    }

    // Print parsed series
    for series in series_list {
        print_resistor_series(&series);
        println!();
    }

    Ok(())
}

fn parse_resistor_series(name: &str, node: &kdl::KdlNode) -> Result<ResistorSeries> {
    let children = node
        .children()
        .context("Resistor series node has no children")?;

    let mut manufacturer = None;
    let mut package = None;
    let mut power = None;
    let mut tolerance = None;
    let mut tcr = None;
    let mut temp = None;
    let mut aec_q200 = None;
    let mut standard_values = Vec::new();
    let mut ranges = Vec::new();
    let mut mpn_template = None;

    for child in children.nodes() {
        match child.name().value() {
            "manufacturer" => {
                manufacturer = child
                    .entries()
                    .first()
                    .and_then(|e| e.value().as_string())
                    .map(|s| s.to_string());
            }
            "package" => {
                package = child
                    .entries()
                    .first()
                    .and_then(|e| e.value().as_string())
                    .map(|s| s.to_string());
            }
            "power" => {
                let value = child
                    .entries()
                    .first()
                    .and_then(|e| Decimal::from_str(&e.value().to_string()).ok())
                    .context("Missing power value")?;
                power = Some(PowerSpec {
                    value,
                    unit: "W".to_string(),
                });
            }
            "tolerance" => {
                tolerance = child
                    .entries()
                    .first()
                    .and_then(|e| Decimal::from_str(&e.value().to_string()).ok())
                    .map(|d| d.to_string().parse::<f64>().unwrap());
            }
            "tcr" => {
                let value = child
                    .entries()
                    .first()
                    .and_then(|e| Decimal::from_str(&e.value().to_string()).ok())
                    .context("Missing TCR value")?;
                tcr = Some(TcrSpec {
                    value,
                    unit: "1/K".to_string(),
                });
            }
            "temp" => {
                if let Some(temp_children) = child.children() {
                    let mut min_val = None;
                    let mut max_val = None;

                    for temp_child in temp_children.nodes() {
                        match temp_child.name().value() {
                            "min" => {
                                min_val = temp_child
                                    .entries()
                                    .first()
                                    .and_then(|e| Decimal::from_str(&e.value().to_string()).ok())
                                    .map(|d| d.to_string().parse::<f64>().unwrap());
                            }
                            "max" => {
                                max_val = temp_child
                                    .entries()
                                    .first()
                                    .and_then(|e| Decimal::from_str(&e.value().to_string()).ok())
                                    .map(|d| d.to_string().parse::<f64>().unwrap());
                            }
                            _ => {}
                        }
                    }

                    if let (Some(min), Some(max)) = (min_val, max_val) {
                        temp = Some(TempRange {
                            min,
                            max,
                            unit: "°C".to_string(),
                        });
                    }
                }
            }
            "aec-q200" => {
                aec_q200 = child
                    .entries()
                    .first()
                    .and_then(|e| e.value().to_string().parse::<u8>().ok());
            }
            "standard-values" => {
                for entry in child.entries() {
                    if let Some(val) = entry.value().as_string() {
                        standard_values.push(val.to_string());
                    }
                }
            }
            "range" => {
                let entries = child.entries();
                if entries.len() >= 2 {
                    if let (Some(min), Some(max)) = (
                        Decimal::from_str(&entries[0].value().to_string()).ok(),
                        Decimal::from_str(&entries[1].value().to_string()).ok(),
                    ) {
                        ranges.push(ResistorRange {
                            min,
                            max,
                            unit: Some("Ω".to_string()),
                        });
                    }
                }
            }
            "mpn_template" => {
                mpn_template = child
                    .entries()
                    .first()
                    .and_then(|e| e.value().as_string())
                    .map(|s| s.to_string());
            }
            _ => {}
        }
    }

    Ok(ResistorSeries {
        name: name.to_string(),
        manufacturer: manufacturer.context("Missing manufacturer")?,
        package: package.context("Missing package")?,
        power: power.context("Missing power spec")?,
        tolerance: tolerance.context("Missing tolerance")?,
        tcr: tcr.context("Missing TCR spec")?,
        temp: temp.context("Missing temperature range")?,
        aec_q200,
        standard_values,
        ranges,
        mpn_template: mpn_template.context("Missing MPN template")?,
    })
}

fn get_standard_values(series_name: &str) -> Vec<Decimal> {
    let base_values = match series_name {
        "E24" => vec![
            "1.0", "1.1", "1.2", "1.3", "1.5", "1.6", "1.8", "2.0", "2.2", "2.4", "2.7", "3.0",
            "3.3", "3.6", "3.9", "4.3", "4.7", "5.1", "5.6", "6.2", "6.8", "7.5", "8.2", "9.1",
        ],
        "E96" => vec![
            "1.00", "1.02", "1.05", "1.07", "1.10", "1.13", "1.15", "1.18", "1.21", "1.24", "1.27",
            "1.30", "1.33", "1.37", "1.40", "1.43", "1.47", "1.50", "1.54", "1.58", "1.62", "1.65",
            "1.69", "1.74", "1.78", "1.82", "1.87", "1.91", "1.96", "2.00", "2.05", "2.10", "2.15",
            "2.21", "2.26", "2.32", "2.37", "2.43", "2.49", "2.55", "2.61", "2.67", "2.74", "2.80",
            "2.87", "2.94", "3.01", "3.09", "3.16", "3.24", "3.32", "3.40", "3.48", "3.57", "3.65",
            "3.74", "3.83", "3.92", "4.02", "4.12", "4.22", "4.32", "4.42", "4.53", "4.64", "4.75",
            "4.87", "4.99", "5.11", "5.23", "5.36", "5.49", "5.62", "5.76", "5.90", "6.04", "6.19",
            "6.34", "6.49", "6.65", "6.81", "6.98", "7.15", "7.32", "7.50", "7.68", "7.87", "8.06",
            "8.25", "8.45", "8.66", "8.87", "9.09", "9.31", "9.53", "9.76",
        ],
        _ => vec![],
    };

    base_values
        .into_iter()
        .map(|s| Decimal::from_str(s).unwrap())
        .collect()
}

fn generate_resistor_values(standard_series: &[String]) -> Vec<Decimal> {
    let mut all_values = Vec::new();

    for series_name in standard_series {
        let base_values = get_standard_values(series_name);

        // Generate values across multiple decades (1Ω to 10MΩ)
        for decade in 0..7 {
            // 10^0 to 10^6
            let multiplier = Decimal::from(10_i32.pow(decade as u32));
            for &base in &base_values {
                all_values.push(base * multiplier);
            }
        }
    }

    // Remove duplicates and sort
    all_values.sort();
    all_values.dedup();

    all_values
}

fn filter_by_ranges(values: &[Decimal], ranges: &[ResistorRange]) -> Vec<Decimal> {
    if ranges.is_empty() {
        return values.to_vec();
    }

    values
        .iter()
        .filter(|&&value| {
            ranges
                .iter()
                .any(|range| value >= range.min && value <= range.max)
        })
        .copied()
        .collect()
}

fn format_resistance(value: Decimal) -> String {
    let million = Decimal::from(1_000_000);
    let thousand = Decimal::from(1_000);

    if value >= million {
        let megaohms = value / million;
        let formatted = megaohms.to_string();
        // Remove trailing zeros after decimal point
        let trimmed = if formatted.contains('.') {
            formatted.trim_end_matches('0').trim_end_matches('.')
        } else {
            &formatted
        };
        format!("{}MΩ", trimmed)
    } else if value >= thousand {
        let kiloohms = value / thousand;
        let formatted = kiloohms.to_string();
        // Remove trailing zeros after decimal point
        let trimmed = if formatted.contains('.') {
            formatted.trim_end_matches('0').trim_end_matches('.')
        } else {
            &formatted
        };
        format!("{}kΩ", trimmed)
    } else {
        let formatted = value.to_string();
        let trimmed = if formatted.contains('.') {
            formatted.trim_end_matches('0').trim_end_matches('.')
        } else {
            &formatted
        };
        format!("{}Ω", trimmed)
    }
}

/// Convert resistance value to 4-character ERJ value code (IEC 60062)
pub fn resistance_to_value_code(r: Decimal) -> Result<String> {
    if r <= Decimal::ZERO {
        return Err(anyhow!("Resistance must be > 0"));
    }

    // Branch 1: numeric code (≥ 100 Ω) - 3 significant digits + exponent
    let hundred = Decimal::from(100);
    if r >= hundred {
        let mut int_val = r.round();
        let mut exponent: u32 = 0;

        // Reduce to 3-digit mantissa
        while int_val >= Decimal::from(1000) {
            int_val /= Decimal::from(10);
            exponent += 1;
        }

        let mantissa = int_val
            .to_u32()
            .ok_or_else(|| anyhow!("Invalid mantissa value"))?;
        return Ok(format!("{:03}{}", mantissa, exponent));
    }

    // Branch 2: "R" code (< 100 Ω) - R replaces decimal point
    let r_f64 = r
        .to_f64()
        .ok_or_else(|| anyhow!("Cannot convert resistance to f64"))?;

    let code = if r >= Decimal::from(10) {
        // 10-99.9 Ω: 2 digits before R, 1 after (e.g., 10R0, 33R2)
        format!("{:04.1}", r_f64).replace('.', "R")
    } else if r >= Decimal::from(1) {
        // 1-9.99 Ω: 1 digit before R, 2 after (e.g., 6R81, 4R70)
        format!("{:04.2}", r_f64).replace('.', "R")
    } else if r >= Decimal::from_str("0.1").unwrap() {
        // 0.1-0.999 Ω: 0 before R, 3 after (e.g., 0R330)
        let milliohms = (r * Decimal::from(1000)).round();
        format!("0R{:03}", milliohms.to_u32().unwrap())
    } else {
        // < 0.1 Ω: R followed by 3 digits (e.g., R047)
        let milliohms = (r * Decimal::from(1000)).round();
        format!("R{:03}", milliohms.to_u32().unwrap())
    };

    Ok(code)
}

/// Generate full MPN using simple template
pub fn generate_mpn_from_template(resistance: Decimal, template: &str) -> Result<String> {
    render_template(template, resistance)
}

/// Render a template string with placeholders like {value|iec4}
fn render_template(template: &str, resistance: Decimal) -> Result<String> {
    let mut result = String::new();
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            // Parse placeholder
            let mut placeholder = String::new();
            let mut brace_count = 1;

            while let Some(inner_ch) = chars.next() {
                if inner_ch == '{' {
                    brace_count += 1;
                } else if inner_ch == '}' {
                    brace_count -= 1;
                    if brace_count == 0 {
                        break;
                    }
                }
                placeholder.push(inner_ch);
            }

            if brace_count > 0 {
                return Err(anyhow!("Unclosed placeholder in template"));
            }

            // Process placeholder
            let rendered = render_placeholder(&placeholder, resistance)?;
            result.push_str(&rendered);
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

/// Render a single placeholder like "value|iec4"
fn render_placeholder(placeholder: &str, resistance: Decimal) -> Result<String> {
    let parts: Vec<&str> = placeholder.split('|').collect();
    let key = parts[0];
    let transform = parts.get(1).copied();

    // Only support "value" placeholder
    if key != "value" {
        return Err(anyhow!(
            "Only 'value' placeholder is supported, found: {}",
            key
        ));
    }

    // Apply transform
    match transform {
        Some("iec4") => resistance_to_value_code(resistance),
        Some(other) => Err(anyhow!("Unknown transform: {}", other)),
        None => Ok(resistance.to_string()),
    }
}

fn print_resistor_series(series: &ResistorSeries) {
    println!("Resistor Series: {}", series.name);
    println!("  Manufacturer: {}", series.manufacturer);
    println!("  Package: {}", series.package);
    println!("  Power: {} {}", series.power.value, series.power.unit);
    println!("  Tolerance: ±{}%", series.tolerance * 100.0);
    println!(
        "  TCR: ±{} ppm/°C",
        (series.tcr.value * Decimal::from(1_000_000))
            .to_string()
            .parse::<f64>()
            .unwrap()
    );
    println!(
        "  Temperature: {} to {} {}",
        series.temp.min, series.temp.max, series.temp.unit
    );

    if let Some(aec) = series.aec_q200 {
        println!("  AEC-Q200: Grade {}", aec);
    }

    println!("  Standard Values: {}", series.standard_values.join(", "));

    if !series.ranges.is_empty() {
        println!("  Resistance Ranges:");
        for range in &series.ranges {
            println!(
                "    {} - {}",
                format_resistance(range.min),
                format_resistance(range.max)
            );
        }
    }

    // Generate all possible resistor values
    let all_values = generate_resistor_values(&series.standard_values);
    let filtered_values = filter_by_ranges(&all_values, &series.ranges);

    println!(
        "  Available Resistor Values: {} total",
        filtered_values.len()
    );

    // Show a few example MPNs
    if !filtered_values.is_empty() {
        println!("  Sample MPNs:");
        for &value in filtered_values.iter().take(5) {
            if let Ok(mpn) = generate_mpn_from_template(value, &series.mpn_template) {
                println!("    {} → {}", format_resistance(value), mpn);
            }
        }
        if filtered_values.len() > 5 {
            println!("    ... and {} more", filtered_values.len() - 5);
        }
    }
}
