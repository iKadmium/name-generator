# name-generator

An Axum API that returns random first names for a given gender and birth year,
weighted by popularity, using the ONS
[Top 100 baby names in England and Wales: historical data](https://www.ons.gov.uk/peoplepopulationandcommunity/birthsdeathsandmarriages/livebirths/datasets/babynamesenglandandwalestop100babynameshistoricaldata)
(1904–2024).

```sh
cargo run                    # listens on 0.0.0.0:3000 (override with BIND_ADDR)
cargo test
```

## Endpoints

### `GET /name`

| param       | required | notes                                                      |
|-------------|----------|------------------------------------------------------------|
| `gender`    | yes      | `male` / `female` (also accepts `m`, `boy`, `f`, `girl`)   |
| `year`      | yes      | birth year                                                 |
| `forbidden` | no       | comma-separated names to exclude (case-insensitive)        |

```sh
$ curl 'localhost:3000/name?gender=female&year=1987&forbidden=Sarah,Emma'
{"name":"Laura","gender":"female","year":1987,"dataset_year":1984}
```

### `GET /names`

Same parameters plus `count` (1–100, required). Returns `count` distinct names:
each pick is added to the forbidden list before the next one is drawn.

```sh
$ curl 'localhost:3000/names?gender=male&year=2026&count=3'
{"names":["Theo","Arthur","Jude"],"gender":"male","year":2026,"dataset_year":2024}
```

### Errors

Errors return JSON `{"error": "..."}`:

- `400` for missing or invalid parameters
- `422` when the forbidden list leaves fewer names than requested

## How the data is used

- **Years**: the ONS publishes rankings only every 10 years (1904, 1914, …,
  2024). Each request uses the dataset year nearest to `year` (ties go to the
  earlier year). Years outside the range are clamped. The year actually used
  is returned as `dataset_year`.
- **Weighting**: the dataset gives ranks, not counts. Names are weighted by
  Zipf's law, `weight = 1 / rank`, so the #1 name is 100× as likely as the #100
  name.

## Data pipeline

`data/names.csv` (`gender,year,rank,name`) is generated once from the ONS
workbook and compiled into the binary with `include_str!`:

```sh
pip install openpyxl
python3 scripts/convert_xlsx.py historicalnames2024.xlsx > data/names.csv
```

The script trims trailing whitespace from names, skips blank cells, and keeps
tied ranks (e.g. 2004 boys has two names at rank 100).
