//! Deterministic replacement generation for PII values.
//!
//! Generates consistent cover identities from original PII values using
//! simple hashing. The same input always produces the same output,
//! without requiring random number generation.

/// First names for replacement generation (historical/fictional).
const FIRST_NAMES: &[&str] = &[
    "Alexander",
    "Beatrice",
    "Cassius",
    "Diana",
    "Eleanor",
    "Frederick",
    "Genevieve",
    "Hamilton",
    "Isabella",
    "Jasper",
    "Katherine",
    "Leopold",
    "Madeline",
    "Nathaniel",
    "Ophelia",
    "Percival",
    "Quinn",
    "Rosalind",
    "Sebastian",
    "Theodora",
    "Ulysses",
    "Victoria",
    "Wellington",
    "Xenophon",
    "Yolanda",
    "Zacharias",
    "Ambrose",
    "Cordelia",
    "Desmond",
    "Evangeline",
    "Florence",
    "Gideon",
    "Harriet",
    "Ignatius",
    "Josephine",
    "Kingston",
    "Lavinia",
    "Montgomery",
    "Nicolette",
    "Octavian",
    "Penelope",
    "Quentin",
    "Reginald",
    "Seraphina",
    "Thaddeus",
    "Ursula",
    "Valentine",
    "Winifred",
    "Xander",
    "Yvette",
];

/// Last names for replacement generation (historical/fictional).
const LAST_NAMES: &[&str] = &[
    "Ashworth",
    "Blackwood",
    "Cromwell",
    "Darlington",
    "Fairfax",
    "Gladstone",
    "Hartwell",
    "Ironside",
    "Kingsley",
    "Lancaster",
    "Montague",
    "Northcott",
    "Pemberton",
    "Radcliffe",
    "Sinclair",
    "Thornton",
    "Underwood",
    "Whitmore",
    "Ashford",
    "Beaumont",
    "Carlisle",
    "Drummond",
    "Eastwood",
    "Fitzgerald",
    "Grayson",
    "Holloway",
    "Inglewood",
    "Jennings",
    "Kensington",
    "Lockhart",
    "Merriweather",
    "Nightingale",
    "Ogilvie",
    "Pennington",
    "Queensbury",
    "Ravenswood",
    "Stanhope",
    "Templeton",
    "Vanburen",
    "Wainwright",
    "Abernathy",
    "Burlington",
    "Castleton",
    "Devereaux",
    "Ellington",
    "Foxworth",
    "Grimshaw",
    "Hawthorne",
    "Islington",
    "Jeffords",
];

/// Compute a simple deterministic hash of a string.
///
/// Uses a basic FNV-1a-like hash to spread values evenly across indices.
fn simple_hash(input: &str) -> u64 {
    let mut hash: u64 = 14_695_981_039_346_656_037;
    for byte in input.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
}

/// Suggest a replacement name for a person entity.
///
/// Deterministically picks a first and last name based on hashing
/// the original value. The same input always produces the same output.
#[allow(clippy::cast_possible_truncation)]
pub fn suggest_name(original: &str) -> String {
    let hash = simple_hash(original);
    // Modulo guarantees values fit in usize (list lengths are ~50).
    let first_idx = (hash % FIRST_NAMES.len() as u64) as usize;
    let last_idx = ((hash / FIRST_NAMES.len() as u64) % LAST_NAMES.len() as u64) as usize;
    format!("{} {}", FIRST_NAMES[first_idx], LAST_NAMES[last_idx])
}

/// Suggest a replacement phone number.
///
/// Generates a 555-XXXX number deterministically from the original.
/// The 555 prefix is reserved for fictional use in the US.
pub fn suggest_phone(original: &str) -> String {
    let hash = simple_hash(original);
    let digits = hash % 10_000;
    format!("(555) 555-{digits:04}")
}

/// Suggest a replacement SSN.
///
/// Generates a 900-XX-XXXX number (invalid range per SSA rules)
/// deterministically from the original.
pub fn suggest_ssn(original: &str) -> String {
    let hash = simple_hash(original);
    let middle = (hash % 100) as u32;
    let last = ((hash / 100) % 10_000) as u32;
    format!("900-{middle:02}-{last:04}")
}

/// Suggest a replacement email address.
///
/// Generates a name@example.com address deterministically from the original.
/// The example.com domain is reserved by IANA for documentation use.
#[allow(clippy::cast_possible_truncation)]
pub fn suggest_email(original: &str) -> String {
    let hash = simple_hash(original);
    // Modulo guarantees values fit in usize (list lengths are ~50).
    let first_idx = (hash % FIRST_NAMES.len() as u64) as usize;
    let last_idx = ((hash / FIRST_NAMES.len() as u64) % LAST_NAMES.len() as u64) as usize;
    let first = FIRST_NAMES[first_idx].to_lowercase();
    let last = LAST_NAMES[last_idx].to_lowercase();
    format!("{first}.{last}@example.com")
}

/// Suggest a replacement account number.
///
/// Preserves the format (separators like dashes and spaces) but replaces
/// all digits deterministically.
pub fn suggest_account_number(original: &str) -> String {
    let hash = simple_hash(original);
    let mut digit_seed = hash;
    let mut result = String::with_capacity(original.len());

    for ch in original.chars() {
        if ch.is_ascii_digit() {
            let digit = (digit_seed % 10) as u8;
            result.push(char::from(b'0' + digit));
            digit_seed /= 10;
            if digit_seed == 0 {
                digit_seed = simple_hash(&result);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Suggest a replacement value based on the entity type.
///
/// Dispatches to the appropriate specialized generator based on
/// the entity type string. Falls back to a generic name replacement
/// for unrecognized types.
/// Suggest a replacement address.
///
/// Generates a plausible fake address deterministically from the original.
/// Detects whether the input looks like a street address or a city/state/zip
/// and generates an appropriate replacement in the same format.
#[allow(clippy::cast_possible_truncation)]
pub fn suggest_address(original: &str) -> String {
    static STREETS: &[&str] = &[
        "Maple", "Oak", "Cedar", "Elm", "Pine", "Birch", "Walnut", "Cherry", "Willow", "Spruce",
        "Ash", "Holly", "Laurel", "Hazel", "Ivy",
    ];
    static STREET_TYPES: &[&str] = &["St", "Ave", "Blvd", "Dr", "Ln", "Rd", "Ct", "Pl", "Way"];
    static CITIES: &[&str] = &[
        "Springfield",
        "Riverdale",
        "Fairview",
        "Madison",
        "Georgetown",
        "Clinton",
        "Arlington",
        "Burlington",
        "Chester",
        "Franklin",
        "Greenville",
        "Lexington",
        "Milton",
        "Oakland",
        "Salem",
    ];
    static STATES: &[&str] = &[
        "CA", "TX", "FL", "NY", "IL", "PA", "OH", "GA", "NC", "MI", "NJ", "VA", "WA", "AZ", "MA",
    ];

    let hash = simple_hash(original);

    // Detect if this looks like a city/state/zip pattern
    let has_state_code = STATES.iter().any(|s| original.contains(s))
        || original.chars().filter(char::is_ascii_digit).count() >= 5;
    let has_street_number = original
        .split_whitespace()
        .next()
        .is_some_and(|w| w.bytes().all(|b| b.is_ascii_digit()));

    if has_state_code && !has_street_number {
        // City, State ZIP format
        let city_idx = (hash % CITIES.len() as u64) as usize;
        let state_idx = ((hash / CITIES.len() as u64) % STATES.len() as u64) as usize;
        let zip = ((hash / 100) % 90_000) + 10_000;
        format!("{}, {} {zip}", CITIES[city_idx], STATES[state_idx])
    } else {
        // Street address format
        let number = (hash % 900) + 100;
        let street_idx = ((hash / 1000) % STREETS.len() as u64) as usize;
        let type_idx = ((hash / 100_000) % STREET_TYPES.len() as u64) as usize;
        // Check if original has an apt/unit
        let has_apt = original.to_uppercase().contains("APT")
            || original.to_uppercase().contains("UNIT")
            || original.to_uppercase().contains("STE");
        if has_apt {
            let apt_num = (hash % 20) + 1;
            format!(
                "{number} {} {} Apt {apt_num}",
                STREETS[street_idx], STREET_TYPES[type_idx]
            )
        } else {
            format!(
                "{number} {} {}",
                STREETS[street_idx], STREET_TYPES[type_idx]
            )
        }
    }
}

pub fn suggest_replacement(original: &str, entity_type: &str) -> String {
    match entity_type {
        "PHONE" => suggest_phone(original),
        "SSN" => suggest_ssn(original),
        "EMAIL" => suggest_email(original),
        "ACCOUNT_NUMBER" | "CREDIT_CARD" => suggest_account_number(original),
        "ADDRESS" => suggest_address(original),
        // PERSON, NAME, and all other unrecognized types default to a name replacement.
        _ => suggest_name(original),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggest_name_deterministic() {
        let result1 = suggest_name("John Smith");
        let result2 = suggest_name("John Smith");
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_suggest_name_different_inputs_different_outputs() {
        let result1 = suggest_name("John Smith");
        let result2 = suggest_name("Jane Doe");
        assert_ne!(result1, result2);
    }

    #[test]
    fn test_suggest_phone_deterministic() {
        let result1 = suggest_phone("(555) 867-5309");
        let result2 = suggest_phone("(555) 867-5309");
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_suggest_phone_format() {
        let result = suggest_phone("(555) 867-5309");
        assert!(result.starts_with("(555) 555-"));
        assert_eq!(result.len(), "(555) 555-XXXX".len());
    }

    #[test]
    fn test_suggest_ssn_deterministic() {
        let result1 = suggest_ssn("123-45-6789");
        let result2 = suggest_ssn("123-45-6789");
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_suggest_ssn_format() {
        let result = suggest_ssn("123-45-6789");
        assert!(result.starts_with("900-"));
        // Format: 900-XX-XXXX
        let parts: Vec<&str> = result.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "900");
        assert_eq!(parts[1].len(), 2);
        assert_eq!(parts[2].len(), 4);
    }

    #[test]
    fn test_suggest_email_deterministic() {
        let result1 = suggest_email("john.smith@firstnational.com");
        let result2 = suggest_email("john.smith@firstnational.com");
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_suggest_email_format() {
        let result = suggest_email("test@example.org");
        assert!(result.ends_with("@example.com"));
        assert!(result.contains('.'));
    }

    #[test]
    fn test_suggest_account_number_preserves_format() {
        let result = suggest_account_number("4521-8834-2211");
        // Should preserve dashes
        let parts: Vec<&str> = result.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].len(), 4);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        // All parts should be digits
        for part in parts {
            assert!(part.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn test_suggest_account_number_deterministic() {
        let result1 = suggest_account_number("4521-8834-2211");
        let result2 = suggest_account_number("4521-8834-2211");
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_suggest_replacement_dispatches_correctly() {
        let ssn = suggest_replacement("123-45-6789", "SSN");
        assert!(ssn.starts_with("900-"));

        let phone = suggest_replacement("555-123-4567", "PHONE");
        assert!(phone.starts_with("(555) 555-"));

        let email = suggest_replacement("test@example.com", "EMAIL");
        assert!(email.ends_with("@example.com"));

        let account = suggest_replacement("1234-5678", "ACCOUNT_NUMBER");
        assert!(account.contains('-'));
        assert_eq!(account.len(), "1234-5678".len());
    }
}
