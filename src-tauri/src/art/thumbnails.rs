use std::collections::HashMap;

/// Best-effort mapping from a system's folder name (case-insensitive) to its
/// libretro-thumbnails GitHub repo slug — one repo per system, named after the
/// official/No-Intro system name with spaces and punctuation replaced by underscores.
fn thumbnails_repo(folder_name: &str) -> Option<&'static str> {
    let key = folder_name.to_lowercase();
    let map: HashMap<&str, &str> = HashMap::from([
        ("nes", "Nintendo_-_Nintendo_Entertainment_System"),
        ("nintendo entertainment system", "Nintendo_-_Nintendo_Entertainment_System"),
        ("snes", "Nintendo_-_Super_Nintendo_Entertainment_System"),
        ("super nintendo entertainment system", "Nintendo_-_Super_Nintendo_Entertainment_System"),
        ("n64", "Nintendo_-_Nintendo_64"),
        ("nintendo 64", "Nintendo_-_Nintendo_64"),
        ("gb", "Nintendo_-_Game_Boy"),
        ("game boy", "Nintendo_-_Game_Boy"),
        ("gbc", "Nintendo_-_Game_Boy_Color"),
        ("game boy color", "Nintendo_-_Game_Boy_Color"),
        ("gba", "Nintendo_-_Game_Boy_Advance"),
        ("game boy advance", "Nintendo_-_Game_Boy_Advance"),
        ("gc", "Nintendo_-_GameCube"),
        ("gamecube", "Nintendo_-_GameCube"),
        ("nintendo gamecube", "Nintendo_-_GameCube"),
        ("wii", "Nintendo_-_Wii"),
        ("wiiu", "Nintendo_-_Wii_U"),
        ("wii u", "Nintendo_-_Wii_U"),
        ("ds", "Nintendo_-_Nintendo_DS"),
        ("nintendo ds", "Nintendo_-_Nintendo_DS"),
        ("3ds", "Nintendo_-_Nintendo_3DS"),
        ("nintendo 3ds", "Nintendo_-_Nintendo_3DS"),
        ("psx", "Sony_-_PlayStation"),
        ("ps1", "Sony_-_PlayStation"),
        ("playstation", "Sony_-_PlayStation"),
        ("ps2", "Sony_-_PlayStation_2"),
        ("playstation 2", "Sony_-_PlayStation_2"),
        ("psp", "Sony_-_PlayStation_Portable"),
        ("genesis", "Sega_-_Mega_Drive_-_Genesis"),
        ("megadrive", "Sega_-_Mega_Drive_-_Genesis"),
        ("sega genesis", "Sega_-_Mega_Drive_-_Genesis"),
        ("mastersystem", "Sega_-_Master_System_-_Mark_III"),
        ("sega master system", "Sega_-_Master_System_-_Mark_III"),
        ("gamegear", "Sega_-_Game_Gear"),
        ("game gear", "Sega_-_Game_Gear"),
        ("saturn", "Sega_-_Saturn"),
        ("dreamcast", "Sega_-_Dreamcast"),
        ("dc", "Sega_-_Dreamcast"),
        ("atari2600", "Atari_-_2600"),
        ("atari 2600", "Atari_-_2600"),
    ]);
    map.get(key.as_str()).copied()
}

pub fn has_known_thumbnail_source(folder_name: &str) -> bool {
    thumbnails_repo(folder_name).is_some()
}

/// libretro-thumbnails replaces filesystem-unsafe characters in the game's
/// display name with underscores when naming the image file.
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '&' | '*' | '/' | ':' | '`' | '<' | '>' | '?' | '\\' | '|' | '"' => '_',
            other => other,
        })
        .collect()
}

/// Percent-encodes a filename for use as a URL path segment, keeping common
/// title punctuation (parens, apostrophes, commas) readable.
fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'(' | b')' | b'!' | b',' | b'\'' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// One client per download job, so connections to GitHub are reused and a
/// stalled request can't hang the job forever. Must be created and dropped
/// off the async runtime (reqwest's blocking client panics otherwise).
pub fn http_client() -> anyhow::Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()?)
}

/// The names of the box art images in each thumbnail repo, fetched the first
/// time an exact name isn't found. One per download job, so a bulk download
/// lists each system once.
#[derive(Default)]
pub struct ThumbnailIndex {
    listings: HashMap<&'static str, Option<Vec<String>>>,
}

impl ThumbnailIndex {
    fn listing(&mut self, client: &reqwest::blocking::Client, repo: &'static str) -> Option<&[String]> {
        self.listings
            .entry(repo)
            .or_insert_with(|| fetch_listing(client, repo).ok())
            .as_deref()
    }
}

/// libretro's thumbnail server mirrors the repos and, unlike GitHub's API,
/// lists a folder without a rate limit.
fn fetch_listing(client: &reqwest::blocking::Client, repo: &str) -> anyhow::Result<Vec<String>> {
    let url = format!(
        "https://thumbnails.libretro.com/{}/Named_Boxarts/",
        encode_path_segment(&repo.replace('_', " "))
    );
    let response = client.get(&url).send()?;
    if !response.status().is_success() {
        anyhow::bail!("couldn't list {} ({})", url, response.status());
    }
    Ok(parse_listing(&response.text()?))
}

/// Image names (without ".png") from an Apache directory index.
fn parse_listing(html: &str) -> Vec<String> {
    html.split("href=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter_map(|href| href.strip_suffix(".png"))
        .filter(|href| !href.contains('/'))
        .map(percent_decode)
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = s.get(i + 1..i + 3).and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Consoles whose games Nintendo re-released as Virtual Console titles, by the
/// tag No-Intro gives them, e.g. "Majora's Mask (USA) (N64) (Virtual Console)".
fn virtual_console_repo(tag: &str) -> Option<&'static str> {
    match tag {
        "NES" | "Famicom" => thumbnails_repo("nes"),
        "SNES" | "Super Famicom" => thumbnails_repo("snes"),
        "N64" => thumbnails_repo("n64"),
        "GB" => thumbnails_repo("gb"),
        "GBC" => thumbnails_repo("gbc"),
        "GBA" => thumbnails_repo("gba"),
        "DS" => thumbnails_repo("ds"),
        "Wii" => thumbnails_repo("wii"),
        "Genesis" | "Mega Drive" => thumbnails_repo("genesis"),
        _ => None,
    }
}

/// The title without its (…) and […] tags.
fn title_of(name: &str) -> String {
    let mut title = String::new();
    let mut depth = 0i32;
    for c in name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            c if depth == 0 => title.push(c),
            _ => {}
        }
    }
    title
}

/// Lower-case letters and digits only, so "&" (saved as "_"), spacing and
/// punctuation don't matter.
fn comparable(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn tags_of(name: &str) -> Vec<String> {
    name.split('(')
        .skip(1)
        .filter_map(|rest| rest.split(')').next())
        .map(|t| t.trim().to_string())
        .collect()
}

const REGIONS: &[&str] = &[
    "World", "USA", "Europe", "Japan", "Asia", "Australia", "Korea", "China", "Taiwan", "Hong Kong", "Brazil",
    "Canada", "France", "Germany", "Spain", "Italy", "Netherlands", "Sweden", "Scandinavia", "UK", "Russia",
    "Latin America", "Mexico", "Argentina", "Poland", "Portugal", "Greece", "Denmark", "Norway", "Finland",
];

fn regions_in(tag: &str) -> Option<Vec<&str>> {
    let parts: Vec<&str> = tag.split(',').map(str::trim).collect();
    parts.iter().all(|p| REGIONS.contains(p)).then_some(parts)
}

/// "En", "Fr,De", "Zh-Hant": a language list.
fn is_languages(tag: &str) -> bool {
    tag.split(',').map(str::trim).all(|p| {
        let mut chars = p.chars();
        matches!((chars.next(), chars.next()), (Some(a), Some(b)) if a.is_ascii_uppercase() && b.is_ascii_lowercase())
            && p.len() <= 7
            && p.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
    })
}

fn is_revision(tag: &str) -> bool {
    tag.starts_with("Rev ") || (tag.starts_with('v') && tag[1..].starts_with(|c: char| c.is_ascii_digit()))
}

/// Images that are a different product from the game, even with its title.
fn disqualifies(tag: &str) -> bool {
    let tag = tag.to_lowercase();
    ["beta", "proto", "demo", "kiosk", "debug", "sample", "pirate", "hack", "unl"]
        .iter()
        .any(|word| tag.split(|c: char| !c.is_alphanumeric()).any(|w| w == *word))
}

/// The image in `listing` that best stands in for `game_name` when there's
/// none of that exact name: the same title, sharing a region if any does,
/// with as few extra tags (other editions, re-releases) as possible.
/// Languages and revisions don't change the box, so they barely count.
fn best_match<'a>(game_name: &str, listing: &'a [String]) -> Option<&'a str> {
    let title = comparable(&title_of(game_name));
    best_of(game_name, listing.iter().filter(|candidate| comparable(&title_of(candidate)) == title))
}

/// Titles as file names tend to write them: "The Legend of Zelda" for
/// No-Intro's "Legend of Zelda, The", colons dropped, and so on.
fn loose_title(name: &str) -> String {
    let title = title_of(name).to_lowercase();
    title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && *w != "the")
        .collect()
}

/// Like best_match, for a title taken from a file or folder name rather than
/// a DAT. Titles are compared loosely, and failing that a longer title that
/// ends with this one is accepted when there's only one, which finds
/// "Disney-Pixar Cars 3 - Driven to Win" from "Cars 3 Driven to Win".
fn guessed_match<'a>(name: &str, listing: &'a [String]) -> Option<&'a str> {
    const MIN_SUFFIX_LEN: usize = 10;
    let title = loose_title(name);
    if title.is_empty() {
        return None;
    }
    let same: Vec<&String> = listing.iter().filter(|c| loose_title(c) == title).collect();
    if !same.is_empty() {
        return best_of(name, same.into_iter());
    }
    if title.len() < MIN_SUFFIX_LEN {
        return None;
    }
    let longer: Vec<&String> = listing.iter().filter(|c| loose_title(c).ends_with(&title)).collect();
    let first = loose_title(longer.first()?);
    if longer.iter().any(|c| loose_title(c) != first) {
        return None;
    }
    best_of(name, longer.into_iter())
}

/// Of names already known to share `game_name`'s title, the one that best
/// stands in for it: sharing a region (else USA or World), never a beta,
/// demo or similar the name doesn't ask for, and with the fewest extra tags.
pub(crate) fn best_of<'a>(game_name: &str, candidates: impl Iterator<Item = &'a String>) -> Option<&'a str> {
    if title_of(game_name).trim().is_empty() {
        return None;
    }
    let wanted_tags = tags_of(game_name);
    let wanted_regions: Vec<&str> = wanted_tags.iter().filter_map(|t| regions_in(t)).flatten().collect();

    candidates
        .filter_map(|candidate| {
            // With no region to go by, the library's likeliest one.
            let mut score = if wanted_regions.is_empty()
                && tags_of(candidate)
                    .iter()
                    .filter_map(|t| regions_in(t))
                    .flatten()
                    .any(|r| r == "USA" || r == "World")
            {
                50
            } else {
                0
            };
            for tag in tags_of(candidate) {
                if wanted_tags.contains(&tag) {
                    continue;
                }
                if let Some(regions) = regions_in(&tag) {
                    if regions.iter().any(|r| wanted_regions.contains(r)) {
                        score += 100;
                    }
                } else if disqualifies(&tag) && !wanted_tags.iter().any(|w| w.eq_ignore_ascii_case(&tag)) {
                    return None;
                } else if is_revision(&tag) || is_languages(&tag) {
                    score -= 1;
                } else {
                    score -= 20;
                }
            }
            // A region already equal to the wanted tag was skipped above.
            if tags_of(candidate).iter().any(|t| wanted_tags.contains(t) && regions_in(t).is_some()) {
                score += 100;
            }
            Some((score, candidate))
        })
        .max_by_key(|(score, candidate)| (*score, std::cmp::Reverse(candidate.len())))
        .map(|(_, candidate)| candidate.as_str())
}

fn download(client: &reqwest::blocking::Client, repo: &str, image_name: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let url = format!(
        "https://raw.githubusercontent.com/libretro-thumbnails/{}/master/Named_Boxarts/{}.png",
        repo,
        encode_path_segment(&sanitize_name(image_name))
    );
    let response = client.get(&url).send()?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        anyhow::bail!("{} ({})", url, response.status());
    }
    Ok(Some(response.bytes()?.to_vec()))
}

pub fn fetch_box_art(
    client: &reqwest::blocking::Client,
    index: &mut ThumbnailIndex,
    folder_name: &str,
    game_name: &str,
) -> anyhow::Result<Vec<u8>> {
    let repo = thumbnails_repo(folder_name)
        .ok_or_else(|| anyhow::anyhow!("No known box art source for system \"{}\".", folder_name))?;
    // Updates and DLC share their game's box, which is listed without the tag.
    let game_name = game_name.replace(" (Update)", "").replace(" (DLC)", "");

    // A Virtual Console title's box is with the original console's games.
    let mut sources = Vec::new();
    let tags = tags_of(&game_name);
    if tags.iter().any(|t| t == "Virtual Console") {
        if let Some((tag, vc_repo)) = tags.iter().find_map(|t| virtual_console_repo(t).map(|r| (t, r))) {
            let original = game_name
                .replace(" (Virtual Console)", "")
                .replace(&format!(" ({})", tag), "");
            sources.push((vc_repo, original));
        }
    }
    sources.push((repo, game_name.clone()));

    for (repo, name) in &sources {
        if let Some(bytes) = download(client, repo, name)? {
            return Ok(bytes);
        }
    }
    for (repo, name) in &sources {
        let Some(image) = index.listing(client, repo).and_then(|l| best_match(name, l)).map(str::to_string) else {
            continue;
        };
        if let Some(bytes) = download(client, repo, &image)? {
            return Ok(bytes);
        }
    }
    anyhow::bail!("No box art found for \"{}\"", game_name)
}

/// Box art for a file no DAT names, looked up by the title and region read
/// from its file or folder name. The result is a guess: it may be another
/// release of the game, or occasionally another game.
pub fn guess_box_art(
    client: &reqwest::blocking::Client,
    index: &mut ThumbnailIndex,
    folder_name: &str,
    title: &str,
    region: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    let repo = thumbnails_repo(folder_name)
        .ok_or_else(|| anyhow::anyhow!("No known box art source for system \"{}\".", folder_name))?;
    let title = title.replace(" (Update)", "").replace(" (DLC)", "");
    let name = match region {
        Some(region) => format!("{} ({})", title_of(&title).trim(), region),
        None => title_of(&title).trim().to_string(),
    };
    let listing = index
        .listing(client, repo)
        .ok_or_else(|| anyhow::anyhow!("Couldn't list the box art for \"{}\"", folder_name))?;
    let not_found = || anyhow::anyhow!("No box art found for \"{}\"", name);
    let image = guessed_match(&name, listing).map(str::to_string).ok_or_else(not_found)?;
    download(client, repo, &image)?.ok_or_else(not_found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn listing_names_are_decoded() {
        let html = r#"<a href="?C=N;O=D">Name</a><a href="/Nintendo%20-%20Wii%20U/">Parent</a>
            <a href="Legend%20of%20Zelda%2c%20The%20-%20Majora's%20Mask%20(USA).png">x</a>
            <a href="Mario%20_%20Sonic%20(USA).png">y</a>"#;
        assert_eq!(
            parse_listing(html),
            names(&["Legend of Zelda, The - Majora's Mask (USA)", "Mario _ Sonic (USA)"])
        );
    }

    #[test]
    fn closest_name_shares_title_and_region() {
        let listing = names(&[
            "Legend of Zelda, The - The Wind Waker HD (Europe, Australia) (En,Fr,De,Es,It)",
            "Legend of Zelda, The - The Wind Waker HD (USA, Asia) (En,Fr,Es)",
            "Legend of Zelda, The - Twilight Princess HD (Europe, Australia) (En,Fr,De,Es,It) (Rev 2)",
            "Legend of Zelda, The - Twilight Princess HD (USA) (En,Fr,Es) (Rev 2)",
            "Zelda no Densetsu - Twilight Princess HD (Japan) (Rev 1)",
            "Legend of Zelda, The - Breath of the Wild (USA) (En,Fr,Es)",
        ]);
        let pick = |name| best_match(name, &listing);
        assert_eq!(
            pick("Legend of Zelda, The - The Wind Waker HD (USA) (En,Fr,Es)"),
            Some("Legend of Zelda, The - The Wind Waker HD (USA, Asia) (En,Fr,Es)")
        );
        assert_eq!(
            pick("Legend of Zelda, The - Twilight Princess HD (Europe) (En,Fr,De,Es,It)"),
            Some("Legend of Zelda, The - Twilight Princess HD (Europe, Australia) (En,Fr,De,Es,It) (Rev 2)")
        );
        assert_eq!(
            pick("Legend of Zelda, The - Breath of the Wild (USA)"),
            Some("Legend of Zelda, The - Breath of the Wild (USA) (En,Fr,Es)")
        );
        assert_eq!(pick("Legend of Zelda, The - Skyward Sword (USA)"), None);
    }

    #[test]
    fn other_editions_lose_to_the_plain_release() {
        let listing = names(&[
            "Legend of Zelda, The - Majora's Mask (Europe) (En,Fr,De,Es) (Rev 1) (Wii Virtual Console)",
            "Legend of Zelda, The - Majora's Mask (USA) (Demo) (Kiosk)",
            "Legend of Zelda, The - Majora's Mask (USA) (GameCube)",
            "Legend of Zelda, The - Majora's Mask (USA)",
            "Legend of Zelda, The - Majora's Mask - Redux (USA)",
            "1080 Snowboarding (Europe) (En,Ja,Fr,De)",
            "1080 Snowboarding (Japan, USA) (En,Ja)",
            "1080 Snowboarding (USA) (En,Ja) (LodgeNet)",
        ]);
        assert_eq!(
            best_match("Legend of Zelda, The - Majora's Mask (USA)", &listing),
            Some("Legend of Zelda, The - Majora's Mask (USA)")
        );
        assert_eq!(
            best_match("1080 Snowboarding (USA, Europe)", &listing),
            Some("1080 Snowboarding (Japan, USA) (En,Ja)")
        );
        // Only a demo exists: no box rather than the wrong one.
        assert_eq!(best_match("Some Game (USA)", &names(&["Some Game (USA) (Demo)"])), None);
    }

    #[test]
    fn titles_from_file_names_are_matched_loosely() {
        let listing = names(&[
            "Legend of Zelda, The - Link's Awakening DX (USA, Europe) (Rev 2) (SGB Enhanced)",
            "Legend of Zelda, The - Link's Awakening DX (Germany) (SGB Enhanced)",
            "Goonies II, The (USA)",
            "Goonies (Japan)",
            "Disney-Pixar Cars 3 - Driven to Win (Europe) (En,Fr,De,Es,It,Nl,Pl,Ru)",
            "Disney-Pixar Cars 3 - Driven to Win (USA) (En,Fr,Es,Pt)",
            "Super Pang (USA)",
        ]);
        assert_eq!(
            guessed_match("The Legend of Zelda - Link's Awakening DX", &listing),
            Some("Legend of Zelda, The - Link's Awakening DX (USA, Europe) (Rev 2) (SGB Enhanced)")
        );
        assert_eq!(guessed_match("Goonies, The (Japan)", &listing), Some("Goonies (Japan)"));
        assert_eq!(
            guessed_match("Cars 3 Driven to Win", &listing),
            Some("Disney-Pixar Cars 3 - Driven to Win (USA) (En,Fr,Es,Pt)")
        );
        // Too short to trust a longer title that merely ends with it.
        assert_eq!(guessed_match("Pang (USA)", &listing), None);
        assert_eq!(guessed_match("Pokemon Red Advanced", &listing), None);
    }

    #[test]
    fn virtual_console_tags_name_the_original_console() {
        assert_eq!(virtual_console_repo("N64"), Some("Nintendo_-_Nintendo_64"));
        assert_eq!(virtual_console_repo("DS"), Some("Nintendo_-_Nintendo_DS"));
        assert_eq!(virtual_console_repo("En,Fr"), None);
    }
}
