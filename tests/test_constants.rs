use audex::constants::{GENRES, find_genre_id, get_genre};

// --- get_genre: valid IDs ---

#[test]
fn test_get_genre_valid_ids() {
    // Spot-check representative genres across the full 0-191 range
    assert_eq!(get_genre(0), Some("Blues"));
    assert_eq!(get_genre(17), Some("Rock"));
    assert_eq!(get_genre(52), Some("Electronic"));
    assert_eq!(get_genre(79), Some("Hard Rock"));
    assert_eq!(get_genre(137), Some("Heavy Metal"));
    assert_eq!(get_genre(186), Some("Podcast"));
    assert_eq!(get_genre(191), Some("Psybient"));
}

// --- get_genre: out-of-range IDs ---

#[test]
fn test_get_genre_out_of_range() {
    // First ID beyond the defined table
    assert_eq!(get_genre(192), None);
    assert_eq!(get_genre(200), None);
    assert_eq!(get_genre(255), None);
}

// --- find_genre_id: exact-case matches ---

#[test]
fn test_find_genre_id_exact_match() {
    assert_eq!(find_genre_id("Rock"), Some(17));
    assert_eq!(find_genre_id("Blues"), Some(0));
    assert_eq!(find_genre_id("Electronic"), Some(52));
    assert_eq!(find_genre_id("Podcast"), Some(186));
}

// --- find_genre_id: case-insensitive lookup ---

#[test]
fn test_find_genre_id_case_insensitive() {
    assert_eq!(find_genre_id("rock"), Some(17));
    assert_eq!(find_genre_id("ROCK"), Some(17));
    assert_eq!(find_genre_id("rOcK"), Some(17));
    assert_eq!(find_genre_id("blues"), Some(0));
    assert_eq!(find_genre_id("BLUES"), Some(0));
}

// --- find_genre_id: names not in the standard table ---

#[test]
fn test_find_genre_id_unknown_genre() {
    assert_eq!(find_genre_id("Synthwave"), None);
    assert_eq!(find_genre_id(""), None);
    assert_eq!(find_genre_id("Not A Real Genre"), None);
}

// --- GENRES array: length matches the ID3v1 specification ---

#[test]
fn test_genres_array_length() {
    assert_eq!(GENRES.len(), 192);
}

// --- GENRES array: no entries are blank ---

#[test]
fn test_genres_no_empty_entries() {
    for (i, genre) in GENRES.iter().enumerate() {
        assert!(!genre.is_empty(), "genre at index {} must not be empty", i);
    }
}

// --- Roundtrip: get_genre(id) → find_genre_id(name) must recover the same id ---

#[test]
fn test_get_find_roundtrip() {
    for id in 0u8..192 {
        let name = get_genre(id).unwrap_or_else(|| panic!("get_genre({}) returned None", id));
        let recovered = find_genre_id(name)
            .unwrap_or_else(|| panic!("find_genre_id({:?}) returned None", name));
        assert_eq!(
            recovered, id,
            "roundtrip failed for id {}: get_genre gave {:?}, find_genre_id gave {}",
            id, name, recovered
        );
    }
}
