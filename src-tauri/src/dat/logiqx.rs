use crate::db::repo::{DatGameImport, DatRomImport};
use regex::Regex;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Datafile {
    #[serde(default)]
    header: Option<Header>,
    #[serde(rename = "game", default)]
    games: Vec<Game>,
}

#[derive(Debug, Deserialize, Default)]
struct Header {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Game {
    #[serde(rename = "@name")]
    name: String,
    // Some current No-Intro DATs emit more than one <category> per game;
    // collect them all rather than erroring on a scalar field.
    #[serde(rename = "category", default)]
    category: Vec<String>,
    #[serde(rename = "rom", default)]
    roms: Vec<RomEntry>,
}

#[derive(Debug, Deserialize)]
struct RomEntry {
    #[serde(rename = "@name")]
    name: String,
    // Some DATs (e.g. Wii U sets) emit size="" for entries with no known size —
    // deserialize as a string first so an empty attribute doesn't hard-fail
    // parsing of the whole file, then parse it leniently below.
    #[serde(rename = "@size", default)]
    size: Option<String>,
    #[serde(rename = "@crc", default)]
    crc: Option<String>,
    #[serde(rename = "@md5", default)]
    md5: Option<String>,
    #[serde(rename = "@sha1", default)]
    sha1: Option<String>,
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.is_empty())
}

pub struct ParsedDat {
    pub dat_name: Option<String>,
    pub dat_version: Option<String>,
    pub games: Vec<DatGameImport>,
}

/// Best-effort extraction of a "(Region)" and a 4-digit "(Year)" from a
/// No-Intro/Redump-style game name, e.g. "Super Mario Bros. (USA) (Rev A)".
fn extract_year_region(name: &str) -> (Option<String>, Option<String>) {
    let re = Regex::new(r"\(([^)]+)\)").unwrap();
    let mut year = None;
    let mut region = None;
    for cap in re.captures_iter(name) {
        let val = cap[1].to_string();
        if year.is_none() && val.len() == 4 && val.chars().all(|c| c.is_ascii_digit()) {
            year = Some(val);
        } else if region.is_none() {
            region = Some(val);
        }
    }
    (year, region)
}

pub fn parse_dat(xml: &str) -> anyhow::Result<ParsedDat> {
    let datafile: Datafile = quick_xml::de::from_str(xml)?;
    let dat_name = datafile.header.as_ref().and_then(|h| h.name.clone());
    let dat_version = datafile.header.as_ref().and_then(|h| h.version.clone());

    let games = datafile
        .games
        .into_iter()
        .map(|g| {
            let (year, region) = extract_year_region(&g.name);
            let category = (!g.category.is_empty()).then(|| g.category.join(", "));
            DatGameImport {
                name: g.name,
                category,
                year,
                region,
                roms: g
                    .roms
                    .into_iter()
                    .map(|r| DatRomImport {
                        name: r.name,
                        size: non_empty(r.size).and_then(|s| s.parse::<i64>().ok()),
                        crc32: non_empty(r.crc),
                        md5: non_empty(r.md5),
                        sha1: non_empty(r.sha1),
                    })
                    .collect(),
            }
        })
        .collect();

    Ok(ParsedDat {
        dat_name,
        dat_version,
        games,
    })
}
