//! Constants used across the library, primarily ID3v1 genres
//!
//! This module provides the complete ID3v1 genre table and utilities
//! for parsing and converting genre information across different tagging formats.
//!
//! # ID3v1 Genre System
//!
//! ID3v1 uses a single byte (0-255) to represent genres. The original ID3v1
//! specification defines 80 genres (0-79). Winamp later extended this to
//! 192 genres (0-191), which became the de facto standard. This module provides
//! the complete mapping between numeric IDs and genre names.
//!
//! # Usage Examples
//!
//! ## Basic Genre Lookup
//!
//! ```rust
//! use audex::constants::{get_genre, find_genre_id};
//!
//! // Get genre name by ID
//! let genre = get_genre(17);
//! assert_eq!(genre, Some("Rock"));
//!
//! // Find ID by name (case-insensitive)
//! let id = find_genre_id("rock");
//! assert_eq!(id, Some(17));
//!
//! let id = find_genre_id("ROCK");
//! assert_eq!(id, Some(17));
//! ```
//!
//! ## Custom Genre Handling
//!
//! For genres not in the standard list:
//!
//! ```rust
//! use audex::constants::find_genre_id;
//!
//! // Check if genre is in standard list
//! let custom_genre = "Synthwave";
//! match find_genre_id(custom_genre) {
//!     Some(id) => {
//!         println!("Standard genre ID: {}", id);
//!     }
//!     None => {
//!         println!("Custom genre, use freeform text: {}", custom_genre);
//!         // In ID3v2, you can store this directly as text
//!         // In ID3v1, you'd typically use genre 12 ("Other")
//!     }
//! }
//! ```
//!
//! # Edge Cases and Special Handling
//!
//! ## Out-of-Range IDs
//!
//! ```rust
//! use audex::constants::get_genre;
//!
//! // IDs beyond the defined list return None
//! let genre = get_genre(255);
//! assert_eq!(genre, None);
//!
//! // Always check the result
//! match get_genre(200) {
//!     Some(name) => println!("Genre: {}", name),
//!     None => println!("Unknown genre ID"),
//! }
//! ```
//!
//! ## Case Sensitivity
//!
//! ```rust
//! use audex::constants::find_genre_id;
//!
//! // Genre lookup is case-insensitive
//! assert_eq!(find_genre_id("rock"), find_genre_id("Rock"));
//! assert_eq!(find_genre_id("ROCK"), find_genre_id("RoCk"));
//! ```
//!
//! # Format-Specific Considerations
//!
//! - **ID3v1**: Must use numeric genre ID (0-191), stored as single byte (255 = unset)
//! - **ID3v2**: Can use numeric ID, text, or hybrid format (e.g., "(17)Rock")
//! - **Vorbis/FLAC**: Always freeform text, numeric IDs not used
//! - **MP4/M4A**: Typically numeric for standard genres, can use custom text

/// ID3v1 genre list - complete 192-entry Winamp-extended table (indices 0-191)
pub const GENRES: &[&str] = &[
    // 0-9
    "Blues",
    "Classic Rock",
    "Country",
    "Dance",
    "Disco",
    "Funk",
    "Grunge",
    "Hip-Hop",
    "Jazz",
    "Metal",
    // 10-19
    "New Age",
    "Oldies",
    "Other",
    "Pop",
    "R&B",
    "Rap",
    "Reggae",
    "Rock",
    "Techno",
    "Industrial",
    // 20-29
    "Alternative",
    "Ska",
    "Death Metal",
    "Pranks",
    "Soundtrack",
    "Euro-Techno",
    "Ambient",
    "Trip-Hop",
    "Vocal",
    "Jazz+Funk",
    // 30-39
    "Fusion",
    "Trance",
    "Classical",
    "Instrumental",
    "Acid",
    "House",
    "Game",
    "Sound Clip",
    "Gospel",
    "Noise",
    // 40-49
    "Alt. Rock",
    "Bass",
    "Soul",
    "Punk",
    "Space",
    "Meditative",
    "Instrumental Pop",
    "Instrumental Rock",
    "Ethnic",
    "Gothic",
    // 50-59
    "Darkwave",
    "Techno-Industrial",
    "Electronic",
    "Pop-Folk",
    "Eurodance",
    "Dream",
    "Southern Rock",
    "Comedy",
    "Cult",
    "Gangsta Rap",
    // 60-69
    "Top 40",
    "Christian Rap",
    "Pop/Funk",
    "Jungle",
    "Native American",
    "Cabaret",
    "New Wave",
    "Psychedelic",
    "Rave",
    "Showtunes",
    // 70-79
    "Trailer",
    "Lo-Fi",
    "Tribal",
    "Acid Punk",
    "Acid Jazz",
    "Polka",
    "Retro",
    "Musical",
    "Rock & Roll",
    "Hard Rock",
    // 80-89
    "Folk",
    "Folk-Rock",
    "National Folk",
    "Swing",
    "Fast-Fusion",
    "Bebop",
    "Latin",
    "Revival",
    "Celtic",
    "Bluegrass",
    // 90-99
    "Avantgarde",
    "Gothic Rock",
    "Progressive Rock",
    "Psychedelic Rock",
    "Symphonic Rock",
    "Slow Rock",
    "Big Band",
    "Chorus",
    "Easy Listening",
    "Acoustic",
    // 100-109
    "Humour",
    "Speech",
    "Chanson",
    "Opera",
    "Chamber Music",
    "Sonata",
    "Symphony",
    "Booty Bass",
    "Primus",
    "Porn Groove",
    // 110-119
    "Satire",
    "Slow Jam",
    "Club",
    "Tango",
    "Samba",
    "Folklore",
    "Ballad",
    "Power Ballad",
    "Rhythmic Soul",
    "Freestyle",
    // 120-129
    "Duet",
    "Punk Rock",
    "Drum Solo",
    "A Cappella",
    "Euro-House",
    "Dance Hall",
    "Goa",
    "Drum & Bass",
    "Club-House",
    "Hardcore",
    // 130-139
    "Terror",
    "Indie",
    "BritPop",
    "Afro-Punk",
    "Polsk Punk",
    "Beat",
    "Christian Gangsta Rap",
    "Heavy Metal",
    "Black Metal",
    "Crossover",
    // 140-149
    "Contemporary Christian",
    "Christian Rock",
    "Merengue",
    "Salsa",
    "Thrash Metal",
    "Anime",
    "JPop",
    "Synthpop",
    "Abstract",
    "Art Rock",
    // 150-159
    "Baroque",
    "Bhangra",
    "Big Beat",
    "Breakbeat",
    "Chillout",
    "Downtempo",
    "Dub",
    "EBM",
    "Eclectic",
    "Electro",
    // 160-169
    "Electroclash",
    "Emo",
    "Experimental",
    "Garage",
    "Global",
    "IDM",
    "Illbient",
    "Industro-Goth",
    "Jam Band",
    "Krautrock",
    // 170-179
    "Leftfield",
    "Lounge",
    "Math Rock",
    "New Romantic",
    "Nu-Breakz",
    "Post-Punk",
    "Post-Rock",
    "Psytrance",
    "Shoegaze",
    "Space Rock",
    // 180-189
    "Trop Rock",
    "World Music",
    "Neoclassical",
    "Audiobook",
    "Audio Theatre",
    "Neue Deutsche Welle",
    "Podcast",
    "Indie Rock",
    "G-Funk",
    "Dubstep",
    // 190-191
    "Garage Rock",
    "Psybient",
];

pub fn get_genre(genre_id: u8) -> Option<&'static str> {
    GENRES.get(genre_id as usize).copied()
}

pub fn find_genre_id(name: &str) -> Option<u8> {
    GENRES
        .iter()
        .position(|&genre| genre.eq_ignore_ascii_case(name))
        .and_then(|pos| u8::try_from(pos).ok())
}
