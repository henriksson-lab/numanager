//! Parse nominal passbands out of filter part designations.
//!
//! Necessary because FPbase populates `bandcenter`/`bandwidth` for only about a
//! third of filters, while the designation string carries the same information
//! for nearly all of them. Designations are factual identifiers, so bands
//! derived from them carry no measurement rights -- see
//! `docs/reference/filter_spectra_databases.md`.
//!
//! Vendors write bands very differently:
//!
//! | vendor | example | meaning |
//! |---|---|---|
//! | Zeiss | `BP 470/40` | centre 470, width 40 |
//! | Zeiss | `FT 440/505` | two dichroic edges |
//! | Chroma | `ET480/30x` | centre 480, width 30, exciter |
//! | Chroma | `ZT442/514/561rpc` | three dichroic edges |
//! | Semrock | `FF01-435/40` | centre 435, width 40 |
//! | Semrock | `FF740-Di01` | dichroic edge 740 |
//! | Omega | `580BP20` | centre 580, width 20 |
//! | Alluxa | `520-40 OD6 ULTRA Bandpass` | centre 520, width 40 |
//! | Leica | `P GFP Ex 470/40 (11525314)` | centre 470, width 40 |
//!
//! Rather than six bespoke parsers, this uses one rule set:
//!
//! - a number in [`MIN_CENTER_NM`, `MAX_CENTER_NM`] is a centre or an edge;
//! - a smaller number right after one, separated by `/` or `-`, is its width;
//! - `-` between two centres is a *range* only while the first has no width
//!   yet, which is what separates `BP 390-420` from `Ex 391/32-479/33`;
//! - `/` between two centres lists separate bands, never a range;
//! - the role comes from keywords, and decides what a lone number means.
//!
//! Numbers outside the wavelength range are ignored, which is what keeps
//! catalogue numbers such as Chroma `51018x` from becoming 51018 nm bands.

/// What the filter does, as declared by its prefix or suffix keywords.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Bandpass,
    Longpass,
    Shortpass,
    Dichroic,
    /// Coloured glass, e.g. `G 365`.
    Glass,
}

/// One passband or edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Band {
    Bandpass {
        center_nm: f64,
        width_nm: f64,
    },
    Longpass {
        edge_nm: f64,
    },
    Shortpass {
        edge_nm: f64,
    },
    DichroicEdge {
        edge_nm: f64,
    },
    /// A single named wavelength with no stated width.
    Line {
        center_nm: f64,
    },
}

impl Band {
    /// Half-power range, where the designation states one.
    pub fn range_nm(&self) -> Option<(f64, f64)> {
        match *self {
            Band::Bandpass {
                center_nm,
                width_nm,
            } => Some((center_nm - width_nm / 2.0, center_nm + width_nm / 2.0)),
            _ => None,
        }
    }

    pub fn center_nm(&self) -> f64 {
        match *self {
            Band::Bandpass { center_nm, .. } | Band::Line { center_nm } => center_nm,
            Band::Longpass { edge_nm }
            | Band::Shortpass { edge_nm }
            | Band::DichroicEdge { edge_nm } => edge_nm,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Band::Bandpass { .. } => "bandpass",
            Band::Longpass { .. } => "longpass",
            Band::Shortpass { .. } => "shortpass",
            Band::DichroicEdge { .. } => "dichroic_edge",
            Band::Line { .. } => "line",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Designation {
    pub prefix: String,
    pub role: Role,
    /// How many bands the designation promises: 1 plain, 2 `D`, 3 `T`, 4 `Q`,
    /// 5 `P`. Falls back to the number parsed when no counting prefix is present.
    pub declared_bands: usize,
    pub bands: Vec<Band>,
    /// Zeiss "High Efficiency": steeper edges, higher transmission.
    pub high_efficiency: bool,
}

impl Designation {
    /// True when the number of parsed bands matches what the prefix promised.
    pub fn is_consistent(&self) -> bool {
        self.bands.len() == self.declared_bands
    }
}

/// Plausible centre/edge wavelengths. The observed data spans 191-1800 nm;
/// this is deliberately wider, but still narrow enough to reject catalogue
/// numbers.
pub const MIN_CENTER_NM: f64 = 180.0;
pub const MAX_CENTER_NM: f64 = 2500.0;
/// Upper bound on a bandwidth. Generous, because the real discriminator is
/// that a width is *smaller than the centre it follows*: `855/210` is a 210 nm
/// wide band, while `484/561` is two bands because 561 exceeds 484.
pub const MAX_WIDTH_NM: f64 = 400.0;

/// Vendor tokens that may lead a name; stripped before parsing.
const VENDORS: &[&str] = &[
    "Zeiss",
    "Chroma",
    "Semrock",
    "Omega",
    "Leica",
    "Nikon",
    "Olympus",
    "Alluxa",
    "Thorlabs",
    "ThorLabs",
    "AHF",
    "Lumencor",
    "Andor",
    "Flux",
    "Everix",
    "PerkinElmer",
];

/// Zeiss-style counting prefixes: `DBP` is two bands, `TFT` three edges.
fn counting_prefix(prefix: &str) -> Option<(Role, usize)> {
    let (count, base) = match prefix {
        "BP" | "LP" | "SP" | "KP" | "FT" | "BS" | "G" | "MBS" => (1, prefix),
        // Names beginning with a digit (Omega, Alluxa) have no prefix at all.
        _ if prefix.len() < 2 => return None,
        _ => {
            let (head, tail) = prefix.split_at(1);
            let count = match head {
                "D" => 2,
                "T" => 3,
                "Q" => 4,
                "P" => 5,
                _ => return None,
            };
            (count, tail)
        }
    };
    let role = match base {
        "BP" => Role::Bandpass,
        "LP" => Role::Longpass,
        // Kurzpass, the German shortpass.
        "SP" | "KP" => Role::Shortpass,
        // Farbteiler / main beam splitter.
        "FT" | "BS" | "MBS" => Role::Dichroic,
        "G" => Role::Glass,
        _ => return None,
    };
    Some((role, count))
}

/// Infer the role from keywords anywhere in the designation.
///
/// Order matters: `lpxr` and `dclp` both contain `lp` but are dichroics, so
/// dichroic markers are tested first.
fn infer_role(text: &str) -> Role {
    let lower = text.to_ascii_lowercase();
    const DICHROIC: &[&str] = &[
        "rpc",
        "lpxr",
        "rdc",
        "dclp",
        "dcxr",
        "drlp",
        "beamsplitter",
        "dichroic",
        "splitter",
        "-di0",
        "di0",
        " di ",
        "mbs",
        "bs ",
        "pc",
    ];
    for marker in DICHROIC {
        if lower.contains(marker) {
            return Role::Dichroic;
        }
    }
    if lower.ends_with("bs") || lower.ends_with("dc") || lower.ends_with(" di") {
        return Role::Dichroic;
    }
    if lower.contains("longpass") || lower.ends_with("lp") || lower.contains("lp ") {
        return Role::Longpass;
    }
    if lower.contains("shortpass") || lower.ends_with("sp") || lower.contains("sp ") {
        return Role::Shortpass;
    }
    Role::Bandpass
}

/// A number, plus the character that separated it from the previous token.
#[derive(Debug, Clone, Copy)]
struct Piece {
    value: f64,
    separator: char,
    /// The separator came from an alpha/digit boundary inside one word.
    boundary_code: bool,
}

/// Split into numeric pieces, remembering what separated them.
///
/// Alpha/digit boundaries inside a word become pieces too, so Omega's
/// `580BP20` yields 580 and 20. Such a boundary only counts as a width
/// separator when the letters between look like a type code (`BP`, `QM`), which
/// keeps Semrock's `FF740-Di01` from reading `01` as a width of 740.
fn pieces(text: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut separator = ' ';
    let mut chars = text.char_indices().peekable();
    let bytes = text.as_bytes();

    while let Some((start, ch)) = chars.next() {
        if ch.is_ascii_digit() {
            let mut end = start + 1;
            while end < bytes.len()
                && (bytes[end].is_ascii_digit()
                    || (bytes[end] == b'.' && bytes.get(end + 1).is_some_and(u8::is_ascii_digit)))
            {
                end += 1;
                chars.next();
            }
            if let Ok(value) = text[start..end].parse::<f64>() {
                // Letters immediately before this number, with no separator.
                let preceding: String = text[..start]
                    .chars()
                    .rev()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .collect();
                let boundary_code = !preceding.is_empty()
                    && preceding.len() <= 4
                    && preceding.chars().all(|c| c.is_ascii_uppercase());
                out.push(Piece {
                    value,
                    separator: if preceding.is_empty() {
                        separator
                    } else {
                        '\u{0}'
                    },
                    boundary_code,
                });
            }
            separator = ' ';
        } else if matches!(ch, '/' | '-' | '+' | '_') {
            separator = ch;
        } else if ch.is_whitespace() || ch == '(' || ch == ')' || ch == ',' {
            separator = ' ';
        }
    }
    out
}

fn clean(name: &str) -> String {
    let mut text = name.trim().to_string();
    for vendor in VENDORS {
        if let Some(rest) = text.strip_prefix(vendor) {
            text = rest.trim().to_string();
            break;
        }
    }
    for marker in ["(HE)", " HE", " LED", " sf", " shift free", " superflat"] {
        text = text.replace(marker, " ");
    }
    // Mount codes trail the optical designation.
    if let Some(at) = text.find("DMR") {
        text.truncate(at);
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse a filter designation into nominal bands.
///
/// Returns `None` when no plausible wavelength is present.
pub fn parse(name: &str) -> Option<Designation> {
    let high_efficiency = name.contains("(HE)") || name.contains(" HE");
    let cleaned = clean(name);
    if cleaned.is_empty() {
        return None;
    }

    // A Zeiss-style leading prefix carries both role and band count; otherwise
    // fall back to keyword inference.
    let leading: String = cleaned
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    let counted = counting_prefix(&leading);
    let role = match counted {
        Some((role, _)) => role,
        None => infer_role(&cleaned),
    };

    let mut bands: Vec<Band> = Vec::new();
    // Whether the last band is still open to receiving a width or a range end.
    let mut open = false;

    for piece in pieces(&cleaned) {
        let is_width_separator = matches!(piece.separator, '/' | '-')
            || (piece.boundary_code && piece.separator == '\0');

        // A number small enough to be a width, attached to an open centre by a
        // width separator, is a width. This is tested before the centre range
        // because the two overlap: `FF01-609/181` has a 181 nm-wide passband,
        // and 181 is also a plausible wavelength on its own.
        if open && is_width_separator && piece.value <= MAX_WIDTH_NM {
            if let Some(Band::Line { center_nm }) = bands.last().copied() {
                if piece.value < center_nm {
                    *bands.last_mut().unwrap() = Band::Bandpass {
                        center_nm,
                        width_nm: piece.value,
                    };
                    open = false;
                    continue;
                }
            }
        }

        if !(MIN_CENTER_NM..=MAX_CENTER_NM).contains(&piece.value) {
            continue;
        }

        // `-` between two centres closes the previous one as a range, but only
        // while it has no width yet. That is what distinguishes `BP 390-420`
        // from Leica's `Ex 391/32-479/33`, where `-` starts a new band.
        if piece.separator == '-' && open && role != Role::Dichroic {
            if let Some(Band::Line { center_nm }) = bands.last().copied() {
                if piece.value > center_nm {
                    *bands.last_mut().unwrap() = Band::Bandpass {
                        center_nm: (center_nm + piece.value) / 2.0,
                        width_nm: piece.value - center_nm,
                    };
                    open = false;
                    continue;
                }
            }
        }

        bands.push(Band::Line {
            center_nm: piece.value,
        });
        open = true;
    }

    if bands.is_empty() {
        return None;
    }

    // Anything still a bare Line takes its meaning from the role.
    for band in &mut bands {
        if let Band::Line { center_nm } = *band {
            *band = match role {
                Role::Longpass => Band::Longpass { edge_nm: center_nm },
                Role::Shortpass => Band::Shortpass { edge_nm: center_nm },
                Role::Dichroic => Band::DichroicEdge { edge_nm: center_nm },
                Role::Bandpass | Role::Glass => Band::Line { center_nm },
            };
        }
    }

    let declared_bands = match counted {
        // A dichroic writes each edge separately, so `FT 440/505` legitimately
        // yields two bands from a single-band prefix.
        Some((Role::Dichroic, _)) => bands.len(),
        Some((_, count)) => count,
        None => bands.len(),
    };

    Some(Designation {
        prefix: leading,
        role,
        declared_bands,
        bands,
        high_efficiency,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bands(name: &str) -> Vec<Band> {
        parse(name).expect(name).bands
    }
    fn bp(center_nm: f64, width_nm: f64) -> Band {
        Band::Bandpass {
            center_nm,
            width_nm,
        }
    }

    #[test]
    fn zeiss_bandpass_center_and_width() {
        assert_eq!(bands("Zeiss BP 470/40"), vec![bp(470.0, 40.0)]);
        assert_eq!(bands("Zeiss BP 470/40")[0].range_nm(), Some((450.0, 490.0)));
    }

    #[test]
    fn zeiss_explicit_range() {
        assert_eq!(bands("Zeiss BP 390-420"), vec![bp(405.0, 30.0)]);
    }

    #[test]
    fn hyphen_is_width_when_second_number_is_smaller() {
        assert_eq!(
            bands("Zeiss DBP 518-25+625-30"),
            vec![bp(518.0, 25.0), bp(625.0, 30.0)]
        );
    }

    #[test]
    fn slash_means_two_edges_on_a_dichroic() {
        assert_eq!(
            bands("Zeiss FT 440/505"),
            vec![
                Band::DichroicEdge { edge_nm: 440.0 },
                Band::DichroicEdge { edge_nm: 505.0 },
            ]
        );
        assert_eq!(
            bands("Zeiss FT 495 (HE)"),
            vec![Band::DichroicEdge { edge_nm: 495.0 }]
        );
    }

    #[test]
    fn zeiss_multiband_and_mixed_segments() {
        let triple = parse("Zeiss TBP 425/29+514/31+632/100").unwrap();
        assert_eq!(triple.declared_bands, 3);
        assert!(triple.is_consistent());
        assert_eq!(
            bands("Zeiss DBP 480/22+LP 530"),
            vec![bp(480.0, 22.0), Band::Line { center_nm: 530.0 }]
        );
    }

    #[test]
    fn zeiss_longpass_shortpass_glass() {
        assert_eq!(
            bands("Zeiss LP 615"),
            vec![Band::Longpass { edge_nm: 615.0 }]
        );
        assert_eq!(
            bands("Zeiss KP 685"),
            vec![Band::Shortpass { edge_nm: 685.0 }]
        );
        assert_eq!(bands("Zeiss G 365"), vec![Band::Line { center_nm: 365.0 }]);
        assert_eq!(
            bands("Zeiss MBS 488"),
            vec![Band::DichroicEdge { edge_nm: 488.0 }]
        );
    }

    #[test]
    fn chroma_exciters_emitters_and_dichroics() {
        assert_eq!(bands("Chroma ET480/30x"), vec![bp(480.0, 30.0)]);
        assert_eq!(bands("Chroma HQ535/50m"), vec![bp(535.0, 50.0)]);
        assert_eq!(bands("Chroma D605/55m"), vec![bp(605.0, 55.0)]);
        assert_eq!(
            bands("Chroma Q625lp"),
            vec![Band::Longpass { edge_nm: 625.0 }]
        );
        // Multi-edge dichroic: every number is an edge, none is a width.
        assert_eq!(
            bands("Chroma ZT442/514/561rpc"),
            vec![
                Band::DichroicEdge { edge_nm: 442.0 },
                Band::DichroicEdge { edge_nm: 514.0 },
                Band::DichroicEdge { edge_nm: 561.0 },
            ]
        );
    }

    #[test]
    fn chroma_catalogue_numbers_are_not_wavelengths() {
        // `51018x` is a set number; 51018 nm is not a filter.
        assert!(parse("Chroma 51018x").is_none());
    }

    #[test]
    fn semrock_series_codes_are_not_bands() {
        // `FF01` and `Di01` must not contribute 1 nm bands.
        assert_eq!(bands("Semrock FF01-435/40"), vec![bp(435.0, 40.0)]);
        assert_eq!(
            bands("Semrock FF740-Di01"),
            vec![Band::DichroicEdge { edge_nm: 740.0 }]
        );
        assert_eq!(
            bands("Semrock FF01-519/LP"),
            vec![Band::Longpass { edge_nm: 519.0 }]
        );
    }

    #[test]
    fn omega_writes_width_after_the_type_code() {
        assert_eq!(bands("Omega 580BP20"), vec![bp(580.0, 20.0)]);
        assert_eq!(bands("Omega 535QM35"), vec![bp(535.0, 35.0)]);
        assert_eq!(
            bands("Omega 630LP"),
            vec![Band::Longpass { edge_nm: 630.0 }]
        );
        assert_eq!(bands("Omega 535-700DBEM"), vec![bp(617.5, 165.0)]);
    }

    #[test]
    fn alluxa_and_leica() {
        assert_eq!(
            bands("Alluxa 520-40 OD6 ULTRA Bandpass Filter"),
            vec![bp(520.0, 40.0)]
        );
        assert_eq!(
            bands("Alluxa 494 ULTRA Longpass Dichroic Beamsplitter"),
            vec![Band::DichroicEdge { edge_nm: 494.0 }]
        );
        assert_eq!(
            bands("Leica P GFP Ex 470/40 (11525314)"),
            vec![bp(470.0, 40.0)]
        );
        // `-` starts a new band once the previous one already has a width.
        assert_eq!(
            bands("Leica P FI/TRITC Em 515/30-590/45 (11525315)"),
            vec![bp(515.0, 30.0), bp(590.0, 45.0)]
        );
    }

    #[test]
    fn wide_passbands_are_widths_not_second_bands() {
        // 181 and 210 are plausible wavelengths, but here they are widths.
        assert_eq!(bands("Semrock FF01-609/181"), vec![bp(609.0, 181.0)]);
        assert_eq!(bands("Semrock FF01-855/210"), vec![bp(855.0, 210.0)]);
        // Still two bands when the second number is too large to be a width.
        assert_eq!(
            bands("Semrock FF01-484/561"),
            vec![
                Band::Line { center_nm: 484.0 },
                Band::Line { center_nm: 561.0 }
            ]
        );
    }

    #[test]
    fn rejects_names_without_wavelengths() {
        assert!(parse("Celesta dichroic penta").is_none());
        assert!(parse("").is_none());
        assert!(parse("Zeiss").is_none());
    }
}
