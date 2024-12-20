// TODO: refactor this heresy

use crate::frecency::FrecencyTracker;
use crate::lsp_item::LspItem;
use mlua::prelude::*;
use mlua::FromLua;
use mlua::Lua;
use std::collections::HashSet;

#[derive(Clone, Hash)]
pub struct FuzzyOptions {
    use_typo_resistance: bool,
    use_frecency: bool,
    use_proximity: bool,
    nearby_words: Option<Vec<String>>,
    min_score: u16,
}

impl FromLua for FuzzyOptions {
    fn from_lua(value: LuaValue, _lua: &'_ Lua) -> LuaResult<Self> {
        if let Some(tab) = value.as_table() {
            let use_typo_resistance: bool = tab.get("use_typo_resistance").unwrap_or_default();
            let use_frecency: bool = tab.get("use_frecency").unwrap_or_default();
            let use_proximity: bool = tab.get("use_proximity").unwrap_or_default();
            let nearby_words: Option<Vec<String>> = tab.get("nearby_words").ok();
            let min_score: u16 = tab.get("min_score").unwrap_or_default();

            Ok(FuzzyOptions {
                use_typo_resistance,
                use_frecency,
                use_proximity,
                nearby_words,
                min_score,
            })
        } else {
            Err(mlua::Error::FromLuaConversionError {
                from: "LuaValue",
                to: "FuzzyOptions".to_string(),
                message: None,
            })
        }
    }
}

pub fn fuzzy(
    needle: String,
    haystack: &Vec<LspItem>,
    frecency: &FrecencyTracker,
    opts: FuzzyOptions,
) -> (Vec<i32>, Vec<u32>) {
    let nearby_words: HashSet<String> = HashSet::from_iter(opts.nearby_words.unwrap_or_default());
    let haystack_labels = haystack
        .iter()
        .map(|s| s.filter_text.clone().unwrap_or(s.label.clone()))
        .collect::<Vec<_>>();

    // Fuzzy match with fzrs
    let options = frizbee::Options {
        prefilter: !opts.use_typo_resistance,
        min_score: opts.min_score,
        stable_sort: false,
        ..Default::default()
    };
    let mut matches = frizbee::match_list(
        &needle,
        &haystack_labels
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
        options,
    );

    // Sort by scores
    let match_scores = matches
        .iter()
        .map(|mtch| {
            let frecency_score = if opts.use_frecency {
                frecency.get_score(&haystack[mtch.index_in_haystack]) as i32
            } else {
                0
            };
            let nearby_words_score = if opts.use_proximity {
                nearby_words
                    .get(&haystack_labels[mtch.index_in_haystack])
                    .map(|_| 2)
                    .unwrap_or(0)
            } else {
                0
            };
            let score_offset = haystack[mtch.index_in_haystack].score_offset;

            (mtch.score as i32) + frecency_score + nearby_words_score + score_offset
        })
        .collect::<Vec<_>>();

    // Find the highest score and filter out matches that are unreasonably lower than it
    if opts.use_typo_resistance {
        let max_score = matches.iter().map(|mtch| mtch.score).max().unwrap_or(0);
        let secondary_min_score = max_score.max(16) - 16;
        matches = matches
            .into_iter()
            .filter(|mtch| mtch.score >= secondary_min_score)
            .collect::<Vec<_>>();
    }

    // Return scores and indices
    (
        matches
            .iter()
            .map(|mtch| match_scores[mtch.index] as i32)
            .collect::<Vec<_>>(),
        matches
            .iter()
            .map(|mtch| mtch.index_in_haystack as u32)
            .collect::<Vec<_>>(),
    )
}
