#!/usr/bin/env python3
"""Regenerate the legacy .xls fixtures used by the code table tests.

The published 地震火山関連コード表 is BIFF8, a format nothing in the Rust
ecosystem writes, so the fixtures are produced here once and committed as
binaries. This script is not part of the build; run it only when the fixture
layout needs to change:

    python -m venv .venv && .venv/bin/pip install xlwt
    .venv/bin/python tests/fixtures/make_code_table_xls.py

The layout mirrors the published sheet: a title row, a group heading row, a
column heading row, then data. Region codes are written as numbers and the other
codes as text, exactly as the real workbook stores them.
"""

from pathlib import Path

import xlwt

HERE = Path(__file__).parent

HEADERS = [
    ["AreaForecastLocalE ・ AreaInformationCity ・ PointSeismicIntensity コード表"],
    ["AreaForecastLocalE", "", "", "AreaInformationCity", "", "", "震度観測点"],
    ["Code", "Name", "ふりがな", "Code", "Name", "ふりがな", "Code", "Name", "ふりがな"],
]

# region code (numeric), region name/kana, city code/name/kana, point code/name/kana.
ROWS = [
    (900, "甲野地方北部", "こうのちほうほくぶ", "0999100", "甲野市", "こうのし",
     "0999100", "甲野市山川", "こうのしやまかわ"),
    (900, "甲野地方北部", "こうのちほうほくぶ", "0999100", "甲野市", "こうのし",
     "0999101", "甲野市服部", "こうのしはっとり"),
    (900, "甲野地方北部", "こうのちほうほくぶ", "0999100", "甲野市", "こうのし",
     "0999120", "甲野市月見", "こうのしつきみ"),
    (900, "甲野地方北部", "こうのちほうほくぶ", "0999300", "乙原町", "おつはらちょう",
     "0999320", "乙原町白樺", "おつはらちょうしらかば"),
    (901, "甲野地方南部", "こうのちほうなんぶ", "0999010", "丙川区", "へいかわく",
     "0999010", "丙川区北二条", "へいかわくきたにじょう"),
]

# A spacer row in the middle: everything after it must still be read.
BLANK_AFTER_ROW_INDEX = 2


def write(path, rows, blank_after=None, trailing_bad_row=False):
    book = xlwt.Workbook(encoding="utf-8")
    sheet = book.add_sheet("24")

    for r, header in enumerate(HEADERS):
        for c, value in enumerate(header):
            if value:
                sheet.write(r, c, value)

    row_index = len(HEADERS)
    for i, row in enumerate(rows):
        for c, value in enumerate(row):
            sheet.write(row_index, c, value)
        row_index += 1
        if blank_after is not None and i == blank_after:
            row_index += 1  # leave the row entirely unwritten

    if trailing_bad_row:
        # Areas filled in but no PointSeismicIntensity code: the reader must
        # refuse rather than quietly drop it.
        sheet.write(row_index, 0, 900)
        sheet.write(row_index, 1, "甲野地方北部")
        sheet.write(row_index, 3, "0999100")
        sheet.write(row_index, 4, "甲野市")

    book.save(str(path))
    print(f"wrote {path}")


def main():
    write(HERE / "code_table_min.xls", ROWS, blank_after=BLANK_AFTER_ROW_INDEX)
    # Same table with 甲野市月見 removed: a station leaving the scope.
    write(
        HERE / "code_table_min_shrunk.xls",
        [row for row in ROWS if row[6] != "0999120"],
    )
    # Same table with 甲野市月見 renamed: identity moved, so an inherited
    # coordinate can no longer be trusted.
    renamed = [
        row[:7] + ("甲野市月見東", "こうのしつきみひがし") if row[6] == "0999120" else row
        for row in ROWS
    ]
    write(HERE / "code_table_min_renamed.xls", renamed)
    write(HERE / "code_table_min_bad_row.xls", ROWS[:1], trailing_bad_row=True)


if __name__ == "__main__":
    main()
