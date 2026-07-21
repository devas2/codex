//! Persistent pet state for the status-line virtual pet.

use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

/// Animal types supported by the status-line pet (cosmetic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AnimalType {
    #[default]
    Cat,
    Dog,
    Rabbit,
    Panda,
    Fox,
}

impl AnimalType {
    pub(crate) fn emoji(self) -> &'static str {
        match self {
            Self::Cat => "🐱",
            Self::Dog => "🐶",
            Self::Rabbit => "🐰",
            Self::Panda => "🐼",
            Self::Fox => "🦊",
        }
    }

    pub(crate) fn all() -> &'static [Self] {
        &[
            Self::Cat,
            Self::Dog,
            Self::Rabbit,
            Self::Panda,
            Self::Fox,
        ]
    }
}

/// Full pet state persisted under `$CODEX_HOME/ccpet/pet-state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PetState {
    pub uuid: String,
    pub energy: f64,
    pub expression: String,
    pub animal_type: AnimalType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    pub pet_name: String,
    pub birth_time: DateTime<Utc>,
    pub last_feed_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_decay_time: Option<DateTime<Utc>>,
    pub total_tokens_consumed: u64,
    pub accumulated_tokens: u64,
    pub total_lifetime_tokens: u64,
    /// Session billable-token cursor used to avoid double-feeding across updates.
    #[serde(default)]
    pub last_fed_session_billable: u64,
}

impl PetState {
    /// Fresh pet with a stable default identity (used for first create + snapshots).
    pub(crate) fn new_default(now: DateTime<Utc>) -> Self {
        let animal = AnimalType::Cat;
        Self {
            uuid: Uuid::new_v4().to_string(),
            energy: crate::status_pet::pet::INITIAL_ENERGY,
            expression: crate::status_pet::pet::EXPRESSION_HAPPY.to_string(),
            animal_type: animal,
            emoji: Some(animal.emoji().to_string()),
            pet_name: "Codex".to_string(),
            birth_time: now,
            last_feed_time: now,
            last_decay_time: Some(now),
            total_tokens_consumed: 0,
            accumulated_tokens: 0,
            total_lifetime_tokens: 0,
            last_fed_session_billable: 0,
        }
    }

    /// Fresh pet with randomized animal/name (used after `/ccpet reset`).
    pub(crate) fn new_random(now: DateTime<Utc>) -> Self {
        let animal = random_animal();
        Self {
            uuid: Uuid::new_v4().to_string(),
            energy: crate::status_pet::pet::INITIAL_ENERGY,
            expression: crate::status_pet::pet::EXPRESSION_HAPPY.to_string(),
            animal_type: animal,
            emoji: Some(animal.emoji().to_string()),
            pet_name: random_pet_name(),
            birth_time: now,
            last_feed_time: now,
            last_decay_time: Some(now),
            total_tokens_consumed: 0,
            accumulated_tokens: 0,
            total_lifetime_tokens: 0,
            last_fed_session_billable: 0,
        }
    }

    pub(crate) fn animal_emoji(&self) -> &str {
        self.emoji
            .as_deref()
            .unwrap_or_else(|| self.animal_type.emoji())
    }
}

const PET_NAMES: &[&str] = &[
    "Fluffy", "Whiskers", "Shadow", "Luna", "Max", "Bella", "Charlie", "Lucy", "Cooper", "Ruby",
    "Milo", "Lily", "Buddy", "Chloe", "Rocky", "小白", "毛毛", "球球", "豆豆", "花花", "咪咪",
    "旺财", "小黑", "雪儿", "糖糖",
];

fn random_pet_name() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hash;
    use std::hash::Hasher;
    let mut hasher = DefaultHasher::new();
    Utc::now().timestamp_nanos_opt().unwrap_or(0).hash(&mut hasher);
    Uuid::new_v4().hash(&mut hasher);
    let idx = (hasher.finish() as usize) % PET_NAMES.len();
    PET_NAMES[idx].to_string()
}

fn random_animal() -> AnimalType {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hash;
    use std::hash::Hasher;
    let mut hasher = DefaultHasher::new();
    Utc::now().timestamp_nanos_opt().unwrap_or(0).hash(&mut hasher);
    Uuid::new_v4().hash(&mut hasher);
    let all = AnimalType::all();
    all[(hasher.finish() as usize) % all.len()]
}
