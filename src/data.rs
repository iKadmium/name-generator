//! The baby-name dataset and weighted sampling over it.
//!
//! The ONS data only publishes ranks (top 100), not counts, so each name is
//! weighted by Zipf's law: `weight = 1 / rank`. Name frequencies follow this
//! distribution closely, so it is a reasonable stand-in for real counts.

use std::collections::{HashMap, HashSet};
use std::fmt;

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

/// Generated from the ONS workbook by `scripts/convert_xlsx.py`.
const NAMES_CSV: &str = include_str!("../data/names.csv");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Gender {
    #[serde(alias = "m", alias = "boy")]
    Male,
    #[serde(alias = "f", alias = "girl")]
    Female,
}

impl Gender {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "male" => Some(Self::Male),
            "female" => Some(Self::Female),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Entry {
    name: String,
    weight: f64,
}

#[derive(Debug)]
struct YearTable {
    year: u16,
    entries: Vec<Entry>,
}

#[derive(Debug)]
pub struct Dataset {
    /// Year tables per gender, sorted by year.
    tables: HashMap<Gender, Vec<YearTable>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PickError {
    /// The forbidden list leaves fewer names than were requested.
    NotEnoughNames { requested: usize, available: usize },
}

impl fmt::Display for PickError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnoughNames {
                requested,
                available,
            } => write!(
                f,
                "requested {requested} name(s) but only {available} remain after excluding forbidden names"
            ),
        }
    }
}

#[derive(Debug)]
pub struct Picked {
    /// The year from the dataset that was actually used.
    pub dataset_year: u16,
    pub names: Vec<String>,
}

impl Dataset {
    /// Load the dataset embedded in the binary.
    pub fn embedded() -> Self {
        Self::from_csv(NAMES_CSV).expect("embedded names.csv is valid")
    }

    /// Parse `gender,year,rank,name` CSV (with a header row).
    pub fn from_csv(csv: &str) -> Result<Self, String> {
        let mut by_key: HashMap<(Gender, u16), Vec<Entry>> = HashMap::new();
        for (i, line) in csv.lines().enumerate().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let err = |what: &str| format!("line {}: {what}: {line:?}", i + 1);
            let fields: Vec<&str> = line.split(',').collect();
            let [gender, year, rank, name] = fields[..] else {
                return Err(err("expected 4 fields"));
            };
            let gender = Gender::parse(gender).ok_or_else(|| err("bad gender"))?;
            let year: u16 = year.parse().map_err(|_| err("bad year"))?;
            let rank: u32 = rank.parse().map_err(|_| err("bad rank"))?;
            if rank == 0 || name.is_empty() {
                return Err(err("rank must be >= 1 and name non-empty"));
            }
            by_key.entry((gender, year)).or_default().push(Entry {
                name: name.to_owned(),
                weight: 1.0 / f64::from(rank),
            });
        }

        let mut tables: HashMap<Gender, Vec<YearTable>> = HashMap::new();
        for ((gender, year), entries) in by_key {
            tables
                .entry(gender)
                .or_default()
                .push(YearTable { year, entries });
        }
        for gender in [Gender::Male, Gender::Female] {
            let list = tables
                .get_mut(&gender)
                .ok_or_else(|| format!("no data for {gender:?}"))?;
            list.sort_by_key(|t| t.year);
        }
        Ok(Self { tables })
    }

    /// The dataset table closest to `year` (ties go to the earlier year).
    fn table(&self, gender: Gender, year: i32) -> &YearTable {
        self.tables[&gender]
            .iter()
            .min_by_key(|t| ((i32::from(t.year) - year).abs(), t.year))
            .expect("every gender has at least one year")
    }

    /// Pick `count` distinct names, weighted by popularity, excluding any in
    /// `forbidden` (case-insensitive).
    pub fn pick<R: Rng + ?Sized>(
        &self,
        gender: Gender,
        year: i32,
        forbidden: &[String],
        count: usize,
        rng: &mut R,
    ) -> Result<Picked, PickError> {
        let table = self.table(gender, year);
        let forbidden: HashSet<String> = forbidden.iter().map(|n| n.to_lowercase()).collect();
        let mut candidates: Vec<&Entry> = table
            .entries
            .iter()
            .filter(|e| !forbidden.contains(&e.name.to_lowercase()))
            .collect();

        if candidates.len() < count {
            return Err(PickError::NotEnoughNames {
                requested: count,
                available: candidates.len(),
            });
        }

        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            let total: f64 = candidates.iter().map(|e| e.weight).sum();
            let mut target = rng.random_range(0.0..total);
            // Fall back to the last candidate in case of float rounding.
            let mut idx = candidates.len() - 1;
            for (i, e) in candidates.iter().enumerate() {
                if target < e.weight {
                    idx = i;
                    break;
                }
                target -= e.weight;
            }
            names.push(candidates.swap_remove(idx).name.clone());
        }

        Ok(Picked {
            dataset_year: table.year,
            names,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    #[test]
    fn embedded_dataset_loads() {
        let ds = Dataset::embedded();
        for g in [Gender::Male, Gender::Female] {
            let years: Vec<u16> = ds.tables[&g].iter().map(|t| t.year).collect();
            assert_eq!(years, (1904..=2024).step_by(10).collect::<Vec<_>>());
            assert!(ds.tables[&g].iter().all(|t| t.entries.len() >= 100));
        }
    }

    #[test]
    fn maps_to_nearest_year() {
        let ds = Dataset::embedded();
        let y = |year| ds.table(Gender::Female, year).year;
        assert_eq!(y(1987), 1984);
        assert_eq!(y(1990), 1994);
        assert_eq!(y(1989), 1984); // tie -> earlier
        assert_eq!(y(1800), 1904);
        assert_eq!(y(2100), 2024);
    }

    #[test]
    fn respects_forbidden_case_insensitively() {
        let ds = Dataset::embedded();
        let table = ds.table(Gender::Male, 1954);
        // Forbid all but one name.
        let keep = &table.entries[50].name;
        let forbidden: Vec<String> = table
            .entries
            .iter()
            .filter(|e| &e.name != keep)
            .map(|e| e.name.to_uppercase())
            .collect();
        let picked = ds
            .pick(Gender::Male, 1954, &forbidden, 1, &mut rng())
            .unwrap();
        assert_eq!(picked.names, vec![keep.clone()]);
    }

    #[test]
    fn multiple_names_are_distinct() {
        let ds = Dataset::embedded();
        let picked = ds.pick(Gender::Female, 2024, &[], 100, &mut rng()).unwrap();
        let unique: HashSet<_> = picked.names.iter().collect();
        assert_eq!(unique.len(), 100);
    }

    #[test]
    fn errors_when_not_enough_names() {
        let ds = Dataset::embedded();
        let err = ds
            .pick(Gender::Female, 2024, &["Olivia".into()], 100, &mut rng())
            .unwrap_err();
        assert_eq!(
            err,
            PickError::NotEnoughNames {
                requested: 100,
                available: 99
            }
        );
    }

    #[test]
    fn weighted_towards_top_ranks() {
        let ds = Dataset::embedded();
        let mut rng = rng();
        let mut top = 0;
        let mut bottom = 0;
        for _ in 0..20_000 {
            let n = &ds.pick(Gender::Male, 1904, &[], 1, &mut rng).unwrap().names[0];
            if n == "William" {
                top += 1; // rank 1
            } else if n == "Jesse" {
                bottom += 1; // rank 100
            }
        }
        // Expected ratio is 100:1; allow plenty of slack.
        assert!(top > bottom * 30, "top={top} bottom={bottom}");
    }
}
