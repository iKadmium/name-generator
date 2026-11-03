#!/usr/bin/env python3
"""One-off conversion of the ONS historical baby names workbook to CSV.

Source: https://www.ons.gov.uk/peoplepopulationandcommunity/birthsdeathsandmarriages/livebirths/datasets/babynamesenglandandwalestop100babynameshistoricaldata

Usage: python3 scripts/convert_xlsx.py historicalnames2024.xlsx > data/names.csv

Output columns: gender,year,rank,name
"""
import csv
import sys

import openpyxl

SHEETS = {"Table_1": "female", "Table_2": "male"}


def main(path: str) -> None:
    wb = openpyxl.load_workbook(path, read_only=True, data_only=True)
    out = csv.writer(sys.stdout, lineterminator="\n")
    out.writerow(["gender", "year", "rank", "name"])
    for sheet, gender in SHEETS.items():
        rows = list(wb[sheet].iter_rows(values_only=True))
        header_idx = next(i for i, r in enumerate(rows) if r[0] == "Rank")
        years = rows[header_idx][1:]
        for row in rows[header_idx + 1 :]:
            rank = row[0]
            if not isinstance(rank, int):
                continue
            for year, name in zip(years, row[1:]):
                name = (name or "").strip().rstrip("*").strip()
                if name:
                    out.writerow([gender, year, rank, name])


if __name__ == "__main__":
    main(sys.argv[1])
