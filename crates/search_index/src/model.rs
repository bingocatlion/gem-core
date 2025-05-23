use primitives::{Asset, AssetMarket, AssetProperties, AssetScore};
use serde::{Deserialize, Serialize};

pub const ASSETS_INDEX_NAME: &str = "assets";
pub const ASSETS_FILTERS: &[&str] = &[
    "asset.chain",
    "asset.tokenId",
    "asset.name",
    "asset.symbol",
    "asset.type",
    "score.rank",
    "properties.isEnabled",
    "market.marketCap",
    "market.marketCapFdv",
    "market.marketCapRank",
    "market.totalVolume",
    "tags",
];
pub const ASSETS_SEARCH_ATTRIBUTES: &[&str] = &["asset.tokenId", "asset.chain", "asset.name", "asset.symbol", "asset.type"];
pub const ASSETS_RANKING_RULES: &[&str] = &[
    "words",
    "typo",
    "score.rank:desc",
    "market.marketCapFdv:desc",
    "proximity",
    "market.marketCapRank:asc",
    "market.marketCap:desc",
    "market.totalVolume:desc",
    "attribute",
    "exactness",
];

pub const ASSETS_SORTS: &[&str] = &["score.rank"];

pub const INDEX_PRIMARY_KEY: &str = "id";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DocumentId {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssetDocument {
    pub id: String,
    pub asset: Asset,
    pub properties: AssetProperties,
    pub score: AssetScore,
    pub market: Option<AssetMarket>,
    pub tags: Option<Vec<String>>,
}

pub fn sanitize_index_primary_id(input: &str) -> String {
    input
        .chars() // Iterate over each character
        .filter(|c| c.is_ascii_alphanumeric())
        .collect() // Collect the result into a String
}
