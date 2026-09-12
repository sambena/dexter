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

pub fn fetch_box_art(client: &reqwest::blocking::Client, folder_name: &str, game_name: &str) -> anyhow::Result<Vec<u8>> {
    let repo = thumbnails_repo(folder_name)
        .ok_or_else(|| anyhow::anyhow!("No known box art source for system \"{}\".", folder_name))?;
    // Updates and DLC share their game's box, which is listed without the tag.
    let game_name = game_name.replace(" (Update)", "").replace(" (DLC)", "");
    let file_name = encode_path_segment(&sanitize_name(&game_name));
    let url = format!(
        "https://raw.githubusercontent.com/libretro-thumbnails/{}/master/Named_Boxarts/{}.png",
        repo, file_name
    );
    let response = client.get(&url).send()?;
    if !response.status().is_success() {
        anyhow::bail!("No box art found for \"{}\" ({})", game_name, response.status());
    }
    Ok(response.bytes()?.to_vec())
}
