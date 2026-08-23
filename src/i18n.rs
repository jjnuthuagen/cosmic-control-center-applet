//! Fluent localisation.
//!
//! Every user-facing string lives in `i18n/<lang>/main.ftl` and is reached
//! through the [`fl!`] macro. Nothing in the widget code should contain a
//! literal the user can read.
//!
//! Translations are compiled in rather than loaded from disk so the applet has
//! no runtime data-path to get wrong. Adding a language means adding an arm to
//! `TRANSLATIONS` below.

// `fluent_bundle::FluentBundle` memoizes formatters behind a `RefCell`, so it is
// neither `Send` nor `Sync` and cannot live in a `OnceLock`. The `concurrent`
// variant swaps that for a thread-safe memoizer; it is marginally slower per
// lookup and completely irrelevant at the handful of strings this applet formats.
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::FluentResource;
use std::sync::OnceLock;
use unic_langid::LanguageIdentifier;

/// `(language tag, main.ftl contents)`. Add new languages here.
const TRANSLATIONS: &[(&str, &str)] = &[("en", include_str!("../i18n/en/main.ftl"))];

const FALLBACK: &str = "en";

static BUNDLE: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();

/// Pick a language from the environment, falling back to English.
///
/// Only the primary subtag is matched (`de_DE.UTF-8` -> `de`), which is enough
/// until a language needs region-specific variants.
fn requested_language() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            let tag = value
                .split(['.', '@'])
                .next()
                .unwrap_or_default()
                .replace('_', "-");
            let primary = tag
                .split('-')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !primary.is_empty() && primary != "c" && primary != "posix" {
                return primary;
            }
        }
    }
    FALLBACK.to_string()
}

fn bundle() -> &'static FluentBundle<FluentResource> {
    BUNDLE.get_or_init(|| {
        let wanted = requested_language();
        let (tag, ftl) = TRANSLATIONS
            .iter()
            .find(|(tag, _)| *tag == wanted)
            .or_else(|| TRANSLATIONS.iter().find(|(tag, _)| *tag == FALLBACK))
            .copied()
            .expect("the fallback language must be present in TRANSLATIONS");

        let langid: LanguageIdentifier = tag.parse().unwrap_or_else(|_| {
            FALLBACK
                .parse()
                .expect("the fallback language tag must be valid")
        });

        let mut bundle = FluentBundle::new_concurrent(vec![langid]);
        // Fluent wraps interpolated values in bidirectional isolation marks.
        // They are invisible but they land in the middle of short panel labels
        // and widen them; we are not mixing scripts, so turn them off.
        bundle.set_use_isolating(false);

        let resource = FluentResource::try_new(ftl.to_string())
            .expect("the embedded main.ftl must parse — this is checked at build time by tests");
        bundle
            .add_resource(resource)
            .expect("the embedded main.ftl must not contain duplicate keys");
        bundle
    })
}

/// Look up `key`, substituting `args`.
///
/// Returns the key itself if it is missing, so a forgotten string shows up as
/// visible nonsense in the UI rather than a panic or an empty label.
pub fn lookup(key: &str, args: Option<&fluent_bundle::FluentArgs>) -> String {
    let bundle = bundle();
    let Some(message) = bundle.get_message(key) else {
        tracing::warn!("missing translation for `{key}`");
        return key.to_string();
    };
    let Some(pattern) = message.value() else {
        tracing::warn!("translation `{key}` has no value");
        return key.to_string();
    };

    let mut errors = Vec::new();
    let formatted = bundle.format_pattern(pattern, args, &mut errors);
    for error in errors {
        tracing::warn!("formatting `{key}`: {error}");
    }
    formatted.into_owned()
}

/// Fetch a localised string: `fl!("wifi")`, or `fl!("battery-percent", percent = 82)`.
#[macro_export]
macro_rules! fl {
    ($key:expr) => {
        $crate::i18n::lookup($key, None)
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = fluent_bundle::FluentArgs::new();
        $(args.set(stringify!($name), $value);)+
        $crate::i18n::lookup($key, Some(&args))
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    // The bundle is built with `expect`, so a malformed or duplicate-keyed
    // main.ftl would panic at first use — i.e. when the panel launches the
    // applet, where nobody would see the message. This test moves that failure
    // to CI.
    #[test]
    fn every_embedded_translation_parses() {
        for (tag, ftl) in TRANSLATIONS {
            let resource =
                FluentResource::try_new(ftl.to_string()).unwrap_or_else(|(_, errors)| {
                    panic!("i18n/{tag}/main.ftl failed to parse: {errors:?}")
                });
            let langid: LanguageIdentifier = tag.parse().unwrap();
            let mut bundle = FluentBundle::new_concurrent(vec![langid]);
            bundle.add_resource(resource).unwrap_or_else(|errors| {
                panic!("i18n/{tag}/main.ftl has duplicate keys: {errors:?}")
            });
        }
    }

    // Catches the common drift where an English key is added and a translation
    // is not, which `lookup` would otherwise paper over at runtime.
    #[test]
    fn translations_cover_the_fallback_keys() {
        let parse = |ftl: &str| {
            let resource = FluentResource::try_new(ftl.to_string()).unwrap();
            let langid: LanguageIdentifier = FALLBACK.parse().unwrap();
            let mut bundle = FluentBundle::new_concurrent(vec![langid]);
            bundle.add_resource(resource).unwrap();
            bundle
        };

        let fallback_ftl = TRANSLATIONS
            .iter()
            .find(|(tag, _)| *tag == FALLBACK)
            .expect("fallback language present")
            .1;
        let keys: Vec<String> = fluent_syntax_keys(fallback_ftl);
        assert!(
            !keys.is_empty(),
            "the fallback translation must define keys"
        );

        for (tag, ftl) in TRANSLATIONS {
            let bundle = parse(ftl);
            for key in &keys {
                assert!(
                    bundle.get_message(key).is_some(),
                    "i18n/{tag}/main.ftl is missing `{key}`"
                );
            }
        }
    }

    /// Message ids are the identifiers at the start of an unindented line.
    fn fluent_syntax_keys(ftl: &str) -> Vec<String> {
        ftl.lines()
            .filter(|line| !line.starts_with([' ', '\t', '#']) && line.contains('='))
            .filter_map(|line| line.split('=').next())
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_string)
            .collect()
    }
}
