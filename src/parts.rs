use std::fs;
use tracing::debug;

#[derive(Debug)]
pub struct PartInfo {
    pub mpns: Vec<String>,
}

async fn parse_parts_kdl(path: &std::path::Path) -> Option<Vec<PartInfo>> {
    debug!("Parsing sources KDL from: {:?}", path);

    let content = fs::read_to_string(path).ok()?;
    let doc: kdl::KdlDocument = content.parse().ok()?;

    let mut parts = Vec::new();

    // Get the parts container
    let parts_node = doc.get("parts")?;
    let parts_children = parts_node.children()?;

    for node in parts_children.nodes() {
        let children = node.children()?;

        let mut mpns = Vec::new();

        for child in children.nodes() {
            if child.name().value() == "mpns" {
                // Get all string values from the mpns node
                for entry in child.entries() {
                    if let Some(mpn) = entry.value().as_string() {
                        mpns.push(mpn.to_string());
                    }
                }
            }
        }

        parts.push(PartInfo { mpns });
    }

    Some(parts)
}

pub async fn get_all_mpns() -> Option<Vec<String>> {
    let parts = parse_parts_kdl(std::path::Path::new("db/sources.kdl")).await?;
    let mut all_mpns = Vec::new();

    for part in parts {
        all_mpns.extend(part.mpns);
    }

    Some(all_mpns)
}
