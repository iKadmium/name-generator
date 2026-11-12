//! The baby-name dataset and random sampling over it.
//!
//! Sampling is uniform by default. With weighting on, names are weighted by
//! popularity. The ONS data only publishes ranks (top 100), not counts, so
//! weighting uses Zipf's law: `weight = 1 / rank`. Name frequencies follow this
//! distribution closely, so it is a reasonable stand-in for real counts.

use std::collections::{HashMap, HashSet};
use std::fmt;

use rand::{Rng, RngExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Generated from the ONS workbook by `scripts/convert_xlsx.py`.
const NAMES_CSV: &str = include_str!("../data/names.csv");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Gender {
    #[serde(alias = "m", alias = "boy")]
    Male,
    #[serde(alias = "f", alias = "girl")]
    Female,
}

impl Gender {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Male => "male",
            Self::Female => "female",
        }
    }

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

/// One name to generate.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
pub struct Person {
    pub gender: Gender,
    /// Year of birth. Mapped to the nearest year in the dataset, which has one
    /// every 10 years from 1904 to 2024.
    pub year: i32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PickError {
    /// Every name for this person's gender and year has been forbidden or
    /// already picked.
    NoNamesLeft {
        index: usize,
        gender: Gender,
        dataset_year: u16,
    },
}

impl fmt::Display for PickError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoNamesLeft {
                index,
                gender,
                dataset_year,
            } => write!(
                f,
                "no {} names left for {dataset_year} (people[{index}]) after excluding forbidden and already-picked names",
                gender.as_str()
            ),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Picked {
    pub name: String,
    pub gender: Gender,
    pub year: i32,
    /// The year from the dataset that was actually used.
    pub dataset_year: u16,
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

    /// Pick one name per person, in order: uniformly at random, or weighted by
    /// popularity if `weighted` is set. Names in `forbidden` (case-insensitive)
    /// are excluded, and each picked name is excluded from the picks after it,
    /// so all returned names are distinct.
    pub fn pick<R: Rng + ?Sized>(
        &self,
        people: &[Person],
        forbidden: &[String],
        weighted: bool,
        rng: &mut R,
    ) -> Result<Vec<Picked>, PickError> {
        let mut forbidden: HashSet<String> = forbidden.iter().map(|n| n.to_lowercase()).collect();
        let mut picked = Vec::with_capacity(people.len());
        for (index, &Person { gender, year }) in people.iter().enumerate() {
            let table = self.table(gender, year);
            let candidates: Vec<&Entry> = table
                .entries
                .iter()
                .filter(|e| !forbidden.contains(&e.name.to_lowercase()))
                .collect();
            let choice = if weighted {
                weighted_choice(&candidates, rng)
            } else {
                uniform_choice(&candidates, rng)
            };
            let entry = choice.ok_or(PickError::NoNamesLeft {
                index,
                gender,
                dataset_year: table.year,
            })?;
            forbidden.insert(entry.name.to_lowercase());
            picked.push(Picked {
                name: entry.name.clone(),
                gender,
                year,
                dataset_year: table.year,
            });
        }
        Ok(picked)
    }
}

fn uniform_choice<'a, R: Rng + ?Sized>(candidates: &[&'a Entry], rng: &mut R) -> Option<&'a Entry> {
    if candidates.is_empty() {
        return None;
    }
    Some(candidates[rng.random_range(0..candidates.len())])
}

fn weighted_choice<'a, R: Rng + ?Sized>(
    candidates: &[&'a Entry],
    rng: &mut R,
) -> Option<&'a Entry> {
    let last = candidates.last()?;
    let total: f64 = candidates.iter().map(|e| e.weight).sum();
    let mut target = rng.random_range(0.0..total);
    for e in candidates {
        if target < e.weight {
            return Some(e);
        }
        target -= e.weight;
    }
    // Only reachable through float rounding.
    Some(last)
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

    fn people(gender: Gender, year: i32, n: usize) -> Vec<Person> {
        vec![Person { gender, year }; n]
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
            .pick(
                &people(Gender::Male, 1954, 1),
                &forbidden,
                false,
                &mut rng(),
            )
            .unwrap();
        assert_eq!(picked[0].name, *keep);
        assert_eq!(picked[0].dataset_year, 1954);
    }

    #[test]
    fn picks_are_distinct() {
        let ds = Dataset::embedded();
        let picked = ds
            .pick(&people(Gender::Female, 2024, 100), &[], false, &mut rng())
            .unwrap();
        let unique: HashSet<_> = picked.iter().map(|p| &p.name).collect();
        assert_eq!(unique.len(), 100);
    }

    #[test]
    fn picks_are_distinct_across_genders_and_years() {
        // "Jordan" is in both 1994 lists; with everything else forbidden it
        // can only be picked once.
        let ds = Dataset::embedded();
        let others: Vec<String> = [Gender::Male, Gender::Female]
            .iter()
            .flat_map(|&g| ds.table(g, 1994).entries.iter())
            .map(|e| e.name.clone())
            .filter(|n| n != "Jordan")
            .collect();
        let people = [
            Person {
                gender: Gender::Male,
                year: 1994,
            },
            Person {
                gender: Gender::Female,
                year: 1994,
            },
        ];
        let err = ds.pick(&people, &others, false, &mut rng()).unwrap_err();
        assert_eq!(
            err,
            PickError::NoNamesLeft {
                index: 1,
                gender: Gender::Female,
                dataset_year: 1994
            }
        );
    }

    #[test]
    fn errors_when_names_run_out() {
        let ds = Dataset::embedded();
        let err = ds
            .pick(
                &people(Gender::Female, 2024, 100),
                &["Olivia".into()],
                false,
                &mut rng(),
            )
            .unwrap_err();
        assert_eq!(
            err,
            PickError::NoNamesLeft {
                index: 99,
                gender: Gender::Female,
                dataset_year: 2024
            }
        );
    }

    /// How often the rank 1 and rank 100 names for 1904 boys come up.
    fn top_and_bottom_counts(weighted: bool) -> (u32, u32) {
        let ds = Dataset::embedded();
        let mut rng = rng();
        let one = people(Gender::Male, 1904, 1);
        let (mut top, mut bottom) = (0, 0);
        for _ in 0..20_000 {
            let n = &ds.pick(&one, &[], weighted, &mut rng).unwrap()[0].name;
            if n == "William" {
                top += 1; // rank 1
            } else if n == "Jesse" {
                bottom += 1; // rank 100
            }
        }
        (top, bottom)
    }

    #[test]
    fn weighted_favours_top_ranks() {
        let (top, bottom) = top_and_bottom_counts(true);
        // Expected ratio is 100:1; allow plenty of slack.
        assert!(top > bottom * 30, "top={top} bottom={bottom}");
    }

    #[test]
    fn unweighted_is_uniform() {
        let (top, bottom) = top_and_bottom_counts(false);
        // Each is expected ~200 times (1 in 100).
        for n in [top, bottom] {
            assert!((140..260).contains(&n), "top={top} bottom={bottom}");
        }
    }
}
