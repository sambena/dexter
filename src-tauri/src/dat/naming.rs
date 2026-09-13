//! Naming unmatched files after the DAT game their file name says they are.
//!
//! A file that matches no dump is often still a copy of a known game: an old
//! GoodTools set's "Goonies, The.nes", a bad dump, a copy with a trainer. Its
//! file name is the only evidence of which game, so the result is a name, not
//! a verification, and is shown as such.

use crate::dat::filename::parse_filename_metadata;
use crate::scanner::wiiu::{comparable_title, dat_name_kind};

/// comparable_title without a leading "the", which file names and DATs
/// don't always agree on ("Goonies, The.nes" is No-Intro's "Goonies").
fn title_key(name: &str) -> String {
    let key = comparable_title(name);
    match key.strip_prefix("the") {
        Some(rest) if !rest.is_empty() => rest.to_string(),
        _ => key,
    }
}

/// "(Disc 2)" and the like, which tell a game's discs apart.
fn disc_of(name: &str) -> Option<String> {
    name.split('(')
        .skip(1)
        .filter_map(|rest| rest.split(')').next())
        .find(|tag| tag.starts_with("Disc ") || tag.starts_with("Side "))
        .map(str::to_string)
}

/// The DAT game (by id) that `file_name` names, from `games` of the file's
/// system: the same title, kind (game, update, DLC) and disc, preferring the
/// file name's region and the plain release. None when no game has the title.
pub fn game_for_file_name<'a>(file_name: &str, games: impl IntoIterator<Item = (i64, &'a str)>) -> Option<i64> {
    let parsed = parse_filename_metadata(file_name);
    let key = title_key(&parsed.title);
    if key.is_empty() {
        return None;
    }
    let kind = dat_name_kind(&parsed.title);
    let disc = disc_of(&parsed.title);

    let candidates: Vec<(i64, String)> = games
        .into_iter()
        .filter(|(_, name)| title_key(name) == key &&dat_name_kind(name) == kind && disc_of(name) == disc)
        .map(|(id, name)| (id, name.to_string()))
        .collect();
    let wanted = match &parsed.region {
        Some(region) => format!("{} ({})", parsed.title, region),
        None => parsed.title.clone(),
    };
    let best = crate::art::thumbnails::best_of(&wanted, candidates.iter().map(|(_, name)| name))?;
    candidates.iter().find(|(_, name)| name == best).map(|(id, _)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NES: &[(i64, &str)] = &[
        (1, "Goonies (Japan)"),
        (2, "Goonies II, The (USA)"),
        (3, "Punch-Out!! (USA)"),
        (4, "Punch-Out!! (Europe)"),
        (5, "Punch-Out!! (USA) (Beta)"),
        (6, "Mike Tyson's Punch-Out!! (USA) (Rev 1)"),
    ];

    const GB: &[(i64, &str)] = &[
        (10, "Bubble Bobble Part 2 (USA, Europe)"),
        (11, "Bubble Bobble Part 2 (Japan)"),
    ];

    const PS: &[(i64, &str)] = &[
        (20, "Final Fantasy VII (USA) (Disc 1)"),
        (21, "Final Fantasy VII (USA) (Disc 2)"),
        (22, "Final Fantasy VII (Europe) (Disc 2)"),
    ];

    #[test]
    fn names_follow_title_and_region() {
        assert_eq!(game_for_file_name("Goonies, The.nes", NES.iter().copied()), Some(1));
        assert_eq!(game_for_file_name("Punch-Out!!.nes", NES.iter().copied()), Some(3));
        assert_eq!(game_for_file_name("Bubble Bobble Part 2 (U) [!].gb", GB.iter().copied()), Some(10));
        assert_eq!(game_for_file_name("Bubble Bobble Part 2 (J).gb", GB.iter().copied()), Some(11));
    }

    #[test]
    fn discs_must_agree() {
        assert_eq!(game_for_file_name("Final Fantasy VII (Europe) (Disc 2).bin", PS.iter().copied()), Some(22));
        assert_eq!(game_for_file_name("Final Fantasy VII (Disc 1).bin", PS.iter().copied()), Some(20));
        assert_eq!(game_for_file_name("Final Fantasy VII (Disc 3).bin", PS.iter().copied()), None);
    }

    #[test]
    fn unknown_titles_stay_unnamed() {
        assert_eq!(game_for_file_name("Pokemon Red Advanced (U) [S][p1][!].gb", GB.iter().copied()), None);
        assert_eq!(game_for_file_name("Goonies III.nes", NES.iter().copied()), None);
    }
}
