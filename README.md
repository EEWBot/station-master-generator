# jma-station-master

## Usage

### マスターを作る

`station.json` の場合

```sh
jma-station-master init \
  --station-json station.json \
  --output station-master.json
```

公開フィードから作る場合

```sh
jma-station-master init \
  --stations-json stations.json \
  --code-table-xlsx 地震火山関連コード表.xlsx \
  --release-id 20260723 \
  --effective-from 2026-07-23T12:00:00+09:00 \
  --output station-master.json
```

### Adding release

`station.json` の場合

```sh
jma-station-master update \
  --previous station-master.json \
  --station-json station-new.json \
  --output station-master.new.json
```

公開フィードから作る場合

```sh
jma-station-master update \
  --previous station-master.json \
  --stations-json stations.json \
  --code-table-xlsx 地震火山関連コード表.xlsx \
  --release-id 20261115 \
  --effective-from 2026-11-15T12:00:00+09:00 \
  --output station-master.new.json
```

### Options

| オプション | 意味                   |
| --- |----------------------|
| `--report <PATH>` | サマリーをJSONで書き出す       |
| `--generated-at <RFC3339>` | `generated_at` を固定する |

## 出力

```jsonc
{
  "schema_version": 1,
  "source_kind": "jma_public",
  "generated_at": "2026-08-28T03:00:00+09:00",

  // index_count は、そのリリース時点での観測点テーブルの長さ。
  "releases": [
    { "id": "20260723", "effective_from": "2026-07-23T12:00:00+09:00", "index_count": 4361 }
  ],

  "stations": [
    {
      "code": "0999010",

      // 観測点が存在するかどうか。
      "lifecycle": [
        { "effective_from": "2026-07-23T12:00:00+09:00", "active": true }
      ],

      // その観測点が何で報じられうるか。存在するかどうかとは別に追跡する。
      // エンコーダーは、スコープが有効な観測点のインデックスにだけ値を書く。
      // 残りの枠は未使用のまま。
      "scope_events": [
        {
          "effective_from": "2026-07-23T12:00:00+09:00",
          "scope": "point_seismic_intensity",
          "enabled": true
        }
      ],

      // 時刻 T で有効な値は、effective_from <= T を満たす最後のリビジョン。
      // effective_until は存在しない。
      "metadata": [
        {
          "effective_from": "2026-07-23T12:00:00+09:00",
          "name": "丙川区北二条",
          "kana": "ヘイカワクキタニジョウ",
          "region": { "code": "901", "name": "甲野地方南部", "kana": "コウノチホウナンブ" },
          "city": { "code": "0999010", "name": "丙川区", "kana": "ヘイカワク" },
          "location": { "latitude": 35.06, "longitude": 135.33 },
          "provider": "jma",
          "provider_detail": null
        }
      ]
    }
  ]
}
```
