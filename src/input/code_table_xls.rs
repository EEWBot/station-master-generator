//! Reader for sheet "24" of 地震火山関連コード表.xls.
//!
//! The file is legacy BIFF8, so `calamine`'s `Xls` reader handles it directly; no
//! external converter is involved.
//!
//! Sheet layout, as measured on the published workbook:
//!
//! ```text
//! row 0  title spanning the sheet
//! row 1  group headings: AreaForecastLocalE | AreaInformationCity | 震度観測点
//! row 2  column headings: Code Name ふりがな   Code Name ふりがな   Code Name ふりがな
//! row 3+ data
//! ```

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use calamine::{Data, Range, Reader, Xls};

use crate::kana;

/// Sheet 24 holds three code systems side by side; these are their column offsets.
const COL_REGION_CODE: u32 = 0;
const COL_REGION_NAME: u32 = 1;
const COL_REGION_KANA: u32 = 2;
const COL_CITY_CODE: u32 = 3;
const COL_CITY_NAME: u32 = 4;
const COL_CITY_KANA: u32 = 5;
const COL_POINT_CODE: u32 = 6;
const COL_POINT_NAME: u32 = 7;
const COL_POINT_KANA: u32 = 8;
const COLUMNS: u32 = 9;

/// Rows 0..=2 are titles and headings.
const FIRST_DATA_ROW: u32 = 3;

pub const SHEET_NAME: &str = "24";

/// One PointSeismicIntensity entry together with the areas it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeTableRow {
    pub region_code: String,
    pub region_name: String,
    pub region_kana: Option<String>,
    pub city_code: String,
    pub city_name: String,
    pub city_kana: Option<String>,
    pub point_code: String,
    pub point_name: String,
    pub point_kana: Option<String>,
}

pub fn load(path: &Path) -> Result<Vec<CodeTableRow>> {
    let mut workbook: Xls<_> = calamine::open_workbook(path)
        .with_context(|| format!("opening code table at {}", path.display()))?;
    let range = workbook
        .worksheet_range(SHEET_NAME)
        .with_context(|| format!("reading sheet {SHEET_NAME:?} of {}", path.display()))?;
    parse_range(&range)
}

pub fn parse_range(range: &Range<Data>) -> Result<Vec<CodeTableRow>> {
    let Some((_, last_row)) = range.end().map(|(row, col)| (col, row)) else {
        bail!("sheet {SHEET_NAME} is empty");
    };

    let mut rows = Vec::new();

    // Walk to the very end of the range. Stopping at the first row without a point
    // code would silently discard everything past a stray note or spacer row.
    for row in FIRST_DATA_ROW..=last_row {
        let cells: Vec<String> = (0..COLUMNS)
            .map(|column| cell_text(range.get_value((row, column)), row, column))
            .collect::<Result<_>>()?;

        if cells.iter().all(String::is_empty) {
            continue;
        }

        let point_code = cells[COL_POINT_CODE as usize].clone();
        if point_code.is_empty() {
            bail!(
                "sheet {SHEET_NAME} row {}: PointSeismicIntensity code is empty but the row \
                 carries other values ({:?}); refusing to guess",
                row + 1,
                cells
            );
        }

        rows.push(CodeTableRow {
            region_code: require(&cells, COL_REGION_CODE, row, "region code")?,
            region_name: require(&cells, COL_REGION_NAME, row, "region name")?,
            // Readings are spelled in hiragana here and in katakana everywhere else.
            region_kana: optional_kana(&cells[COL_REGION_KANA as usize]),
            city_code: require(&cells, COL_CITY_CODE, row, "city code")?,
            city_name: require(&cells, COL_CITY_NAME, row, "city name")?,
            city_kana: optional_kana(&cells[COL_CITY_KANA as usize]),
            point_code,
            point_name: require(&cells, COL_POINT_NAME, row, "PointSeismicIntensity name")?,
            point_kana: optional_kana(&cells[COL_POINT_KANA as usize]),
        });
    }

    check_unique(&rows)?;
    Ok(rows)
}

fn require(cells: &[String], column: u32, row: u32, what: &str) -> Result<String> {
    let value = &cells[column as usize];
    if value.is_empty() {
        bail!("sheet {SHEET_NAME} row {}: {what} is empty", row + 1);
    }
    Ok(value.clone())
}

fn optional_kana(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(kana::to_katakana(value))
    }
}

/// Render a cell as the string the code table means.
///
/// Region codes are stored as numbers, so `100.0` has to come back as `"900"`;
/// city and point codes are stored as text and keep their leading zeros.
fn cell_text(cell: Option<&Data>, row: u32, column: u32) -> Result<String> {
    let text = match cell {
        None | Some(Data::Empty) => String::new(),
        Some(Data::String(s)) => s.trim().to_owned(),
        Some(Data::Int(i)) => i.to_string(),
        Some(Data::Float(f)) => {
            if f.fract() == 0.0 {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        }
        Some(Data::Bool(b)) => b.to_string(),
        Some(other) => bail!(
            "sheet {SHEET_NAME} row {} column {}: unexpected cell {other:?}",
            row + 1,
            column + 1
        ),
    };
    Ok(text)
}

/// Both the code and the name have to be unique: the code is the station identity
/// and the name is the only join key the public feed offers.
fn check_unique(rows: &[CodeTableRow]) -> Result<()> {
    let mut seen_codes: HashMap<&str, usize> = HashMap::new();
    let mut seen_names: HashMap<&str, usize> = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        if let Some(first) = seen_codes.insert(&row.point_code, index) {
            bail!(
                "sheet {SHEET_NAME}: duplicate PointSeismicIntensity code {:?} \
                 (entries {} and {})",
                row.point_code,
                first + 1,
                index + 1
            );
        }
        if let Some(first) = seen_names.insert(&row.point_name, index) {
            bail!(
                "sheet {SHEET_NAME}: duplicate PointSeismicIntensity name {:?} \
                 (entries {} and {}); names are the join key for the public feed",
                row.point_name,
                first + 1,
                index + 1
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{COLUMNS, parse_range};
    use calamine::{Data, Range};

    /// Build a range from a grid of cell texts, mimicking the published sheet.
    fn range(rows: &[Vec<Data>]) -> Range<Data> {
        let mut range = Range::new((0, 0), (rows.len() as u32 - 1, COLUMNS - 1));
        for (r, row) in rows.iter().enumerate() {
            for (c, value) in row.iter().enumerate() {
                range.set_value((r as u32, c as u32), value.clone());
            }
        }
        range
    }

    fn text(cells: [&str; COLUMNS as usize]) -> Vec<Data> {
        cells
            .iter()
            .map(|c| {
                if c.is_empty() {
                    Data::Empty
                } else {
                    Data::String((*c).to_owned())
                }
            })
            .collect()
    }

    fn header() -> Vec<Vec<Data>> {
        vec![
            text(["title", "", "", "", "", "", "", "", ""]),
            text([
                "AreaForecastLocalE",
                "",
                "",
                "AreaInformationCity",
                "",
                "",
                "震度観測点",
                "",
                "",
            ]),
            text([
                "Code",
                "Name",
                "ふりがな",
                "Code",
                "Name",
                "ふりがな",
                "Code",
                "Name",
                "ふりがな",
            ]),
        ]
    }

    fn data_row(point_code: &str, point_name: &str) -> Vec<Data> {
        text([
            "900",
            "甲野地方北部",
            "こうのちほうほくぶ",
            "0999100",
            "甲野市",
            "こうのし",
            point_code,
            point_name,
            "こうのしやまかわ",
        ])
    }

    #[test]
    fn skips_headers_and_normalizes_kana() {
        let mut rows = header();
        rows.push(data_row("0999100", "甲野市山川"));
        let parsed = parse_range(&range(&rows)).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].point_code, "0999100");
        assert_eq!(parsed[0].point_name, "甲野市山川");
        assert_eq!(parsed[0].point_kana.as_deref(), Some("コウノシヤマカワ"));
        assert_eq!(parsed[0].region_kana.as_deref(), Some("コウノチホウホクブ"));
        assert_eq!(parsed[0].city_kana.as_deref(), Some("コウノシ"));
    }

    #[test]
    fn numeric_region_codes_become_integer_strings() {
        let mut rows = header();
        let mut row = data_row("0999100", "甲野市山川");
        // The published workbook stores this column as a number.
        row[0] = Data::Float(900.0);
        rows.push(row);

        let parsed = parse_range(&range(&rows)).unwrap();
        assert_eq!(parsed[0].region_code, "900");
        // Text codes keep their leading zeros.
        assert_eq!(parsed[0].city_code, "0999100");
    }

    #[test]
    fn a_blank_row_does_not_truncate_the_sheet() {
        let mut rows = header();
        rows.push(data_row("0999100", "甲野市山川"));
        rows.push(text([""; COLUMNS as usize]));
        rows.push(data_row("0999101", "甲野市服部"));

        let parsed = parse_range(&range(&rows)).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].point_code, "0999101");
    }

    #[test]
    fn a_row_missing_only_the_point_code_is_rejected() {
        let mut rows = header();
        rows.push(data_row("", "甲野市山川"));
        let err = parse_range(&range(&rows)).unwrap_err().to_string();
        assert!(err.contains("code is empty"), "{err}");
    }

    #[test]
    fn duplicate_codes_are_rejected() {
        let mut rows = header();
        rows.push(data_row("0999100", "甲野市山川"));
        rows.push(data_row("0999100", "甲野市服部"));
        let err = parse_range(&range(&rows)).unwrap_err().to_string();
        assert!(
            err.contains("duplicate PointSeismicIntensity code"),
            "{err}"
        );
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let mut rows = header();
        rows.push(data_row("0999100", "甲野市山川"));
        rows.push(data_row("0999101", "甲野市山川"));
        let err = parse_range(&range(&rows)).unwrap_err().to_string();
        assert!(
            err.contains("duplicate PointSeismicIntensity name"),
            "{err}"
        );
    }
}
