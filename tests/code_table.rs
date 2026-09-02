//! Reading sheet "24" out of a real legacy .xls file.
//!
//! The fixtures are genuine BIFF8 workbooks (see `make_code_table_xls.py`), so
//! these exercise the actual binary path rather than a hand-built cell grid.

mod common;

use common::fixture;
use jma_station_master::input::code_table_xls;

#[test]
fn reads_every_data_row_and_skips_the_headings() {
    let rows = code_table_xls::load(&fixture("code_table_min.xls")).unwrap();

    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].point_code, "0999100");
    assert_eq!(rows[0].point_name, "甲野市山川");
    assert_eq!(rows[0].city_name, "甲野市");
    assert_eq!(rows[0].region_name, "甲野地方北部");
}

#[test]
fn numeric_region_codes_come_back_as_integers() {
    let rows = code_table_xls::load(&fixture("code_table_min.xls")).unwrap();

    // The workbook stores this column as a number, so a naive read yields "900.0".
    assert_eq!(rows[0].region_code, "900");
    assert_eq!(rows[4].region_code, "901");
    // Text columns keep their leading zeros.
    assert_eq!(rows[0].city_code, "0999100");
    assert_eq!(rows[4].point_code, "0999010");
}

#[test]
fn readings_are_normalized_to_katakana() {
    let rows = code_table_xls::load(&fixture("code_table_min.xls")).unwrap();

    assert_eq!(rows[0].point_kana.as_deref(), Some("コウノシヤマカワ"));
    assert_eq!(rows[0].region_kana.as_deref(), Some("コウノチホウホクブ"));
    assert_eq!(rows[0].city_kana.as_deref(), Some("コウノシ"));
    assert_eq!(
        rows[4].point_kana.as_deref(),
        Some("ヘイカワクキタニジョウ")
    );
}

#[test]
fn a_spacer_row_does_not_truncate_the_sheet() {
    // The fixture has a blank row after the third entry. Stopping there would have
    // silently dropped the rest of the code table.
    let rows = code_table_xls::load(&fixture("code_table_min.xls")).unwrap();

    let codes: Vec<&str> = rows.iter().map(|r| r.point_code.as_str()).collect();
    assert_eq!(
        codes,
        ["0999100", "0999101", "0999120", "0999320", "0999010"]
    );
}

#[test]
fn a_row_with_areas_but_no_station_code_is_an_error() {
    let error = code_table_xls::load(&fixture("code_table_min_bad_row.xls"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("code is empty"), "{error}");
    assert!(error.contains("refusing to guess"), "{error}");
}

#[test]
fn a_shrunk_table_is_read_as_a_shorter_table_not_a_truncated_one() {
    let rows = code_table_xls::load(&fixture("code_table_min_shrunk.xls")).unwrap();

    let codes: Vec<&str> = rows.iter().map(|r| r.point_code.as_str()).collect();
    assert_eq!(codes, ["0999100", "0999101", "0999320", "0999010"]);
}
