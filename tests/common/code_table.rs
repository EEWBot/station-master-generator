//! The code table fixtures, written out as real `.xlsx` workbooks.
//!
//! These are built rather than committed so that the tests exercise the whole
//! path a published workbook takes — through `rust_xlsxwriter`, onto disk, and
//! back in through `calamine` — instead of a hand-built cell grid.
//!
//! The layout mirrors the published sheet: a title row, a group heading row, a
//! column heading row, then data. Region codes are written as numbers and the
//! other codes as text, exactly as the real workbook stores them; the reader
//! rejects a numeric city or station code, so that distinction is the point.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rust_xlsxwriter::Workbook;

/// Which of the four tables to read. They differ only in the data rows.
#[derive(Debug, Clone, Copy)]
pub enum CodeTable {
    /// Every station, with a spacer row in the middle.
    Min,
    /// 甲野市月見 removed: a station leaving the scope.
    Shrunk,
    /// 甲野市月見 renamed: identity moved, so an inherited coordinate can no
    /// longer be trusted.
    Renamed,
    /// Areas filled in but no PointSeismicIntensity code, which the reader must
    /// refuse rather than quietly drop.
    BadRow,
}

impl CodeTable {
    fn file_name(self) -> &'static str {
        match self {
            CodeTable::Min => "code_table_min.xlsx",
            CodeTable::Shrunk => "code_table_min_shrunk.xlsx",
            CodeTable::Renamed => "code_table_min_renamed.xlsx",
            CodeTable::BadRow => "code_table_min_bad_row.xlsx",
        }
    }
}

/// Path to the workbook for `kind`, written on first use.
pub fn code_table(kind: CodeTable) -> PathBuf {
    dir().join(kind.file_name())
}

/// The directory holding this process's workbooks.
///
/// All four are written by the same one-shot initializer: tests inside a binary
/// run in parallel, and the process id keeps the test binaries — which Cargo also
/// runs in parallel — from writing over each other.
fn dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();

    DIR.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("code-tables-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch directory is creatable");

        for kind in [
            CodeTable::Min,
            CodeTable::Shrunk,
            CodeTable::Renamed,
            CodeTable::BadRow,
        ] {
            write(&dir.join(kind.file_name()), kind).expect("fixture is writable");
        }

        dir
    })
}

const TITLE: &str = "AreaForecastLocalE ・ AreaInformationCity ・ PointSeismicIntensity コード表";

/// region code (numeric), region name/kana, city code/name/kana, point code/name/kana.
struct Row {
    region_code: u32,
    region_name: &'static str,
    region_kana: &'static str,
    city_code: &'static str,
    city_name: &'static str,
    city_kana: &'static str,
    point_code: &'static str,
    point_name: &'static str,
    point_kana: &'static str,
}

const ROWS: [Row; 5] = [
    Row {
        region_code: 900,
        region_name: "甲野地方北部",
        region_kana: "こうのちほうほくぶ",
        city_code: "0999100",
        city_name: "甲野市",
        city_kana: "こうのし",
        point_code: "0999100",
        point_name: "甲野市山川",
        point_kana: "こうのしやまかわ",
    },
    Row {
        region_code: 900,
        region_name: "甲野地方北部",
        region_kana: "こうのちほうほくぶ",
        city_code: "0999100",
        city_name: "甲野市",
        city_kana: "こうのし",
        point_code: "0999101",
        point_name: "甲野市服部",
        point_kana: "こうのしはっとり",
    },
    Row {
        region_code: 900,
        region_name: "甲野地方北部",
        region_kana: "こうのちほうほくぶ",
        city_code: "0999100",
        city_name: "甲野市",
        city_kana: "こうのし",
        point_code: "0999120",
        point_name: "甲野市月見",
        point_kana: "こうのしつきみ",
    },
    Row {
        region_code: 900,
        region_name: "甲野地方北部",
        region_kana: "こうのちほうほくぶ",
        city_code: "0999300",
        city_name: "乙原町",
        city_kana: "おつはらちょう",
        point_code: "0999320",
        point_name: "乙原町白樺",
        point_kana: "おつはらちょうしらかば",
    },
    Row {
        region_code: 901,
        region_name: "甲野地方南部",
        region_kana: "こうのちほうなんぶ",
        city_code: "0999010",
        city_name: "丙川区",
        city_kana: "へいかわく",
        point_code: "0999010",
        point_name: "丙川区北二条",
        point_kana: "へいかわくきたにじょう",
    },
];

/// The station the shrunk and renamed tables single out.
const MOVED_POINT_CODE: &str = "0999120";

/// A spacer row in the middle of the minimal table: everything after it must
/// still be read.
const BLANK_AFTER_ROW_INDEX: usize = 2;

fn write(path: &Path, kind: CodeTable) -> Result<(), rust_xlsxwriter::XlsxError> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("24")?;

    sheet.write_string(0, 0, TITLE)?;
    for (column, heading) in [
        (0, "AreaForecastLocalE"),
        (3, "AreaInformationCity"),
        (6, "震度観測点"),
    ] {
        sheet.write_string(1, column, heading)?;
    }
    for group in 0..3 {
        let column = group * 3;
        sheet.write_string(2, column, "Code")?;
        sheet.write_string(2, column + 1, "Name")?;
        sheet.write_string(2, column + 2, "ふりがな")?;
    }

    let mut row_index = 3;
    for (position, row) in ROWS.iter().enumerate() {
        match kind {
            CodeTable::Shrunk if row.point_code == MOVED_POINT_CODE => continue,
            CodeTable::BadRow if position > 0 => break,
            _ => {}
        }

        let (point_name, point_kana) = match kind {
            CodeTable::Renamed if row.point_code == MOVED_POINT_CODE => {
                ("甲野市月見東", "こうのしつきみひがし")
            }
            _ => (row.point_name, row.point_kana),
        };

        sheet.write_number(row_index, 0, row.region_code)?;
        sheet.write_string(row_index, 1, row.region_name)?;
        sheet.write_string(row_index, 2, row.region_kana)?;
        // Text, so the leading zero survives the round trip.
        sheet.write_string(row_index, 3, row.city_code)?;
        sheet.write_string(row_index, 4, row.city_name)?;
        sheet.write_string(row_index, 5, row.city_kana)?;
        sheet.write_string(row_index, 6, row.point_code)?;
        sheet.write_string(row_index, 7, point_name)?;
        sheet.write_string(row_index, 8, point_kana)?;
        row_index += 1;

        if matches!(kind, CodeTable::Min) && position == BLANK_AFTER_ROW_INDEX {
            row_index += 1; // leave the row entirely unwritten
        }
    }

    if matches!(kind, CodeTable::BadRow) {
        sheet.write_number(row_index, 0, 900)?;
        sheet.write_string(row_index, 1, "甲野地方北部")?;
        sheet.write_string(row_index, 3, "0999100")?;
        sheet.write_string(row_index, 4, "甲野市")?;
    }

    workbook.save(path)?;
    Ok(())
}
