//! Translittération pure d'un nom en slug d'URL — voir docs/plans/phase-2-api-ui.md, section
//! « assign_slugs ». Aucun accès base : le job décide quoi faire d'un résultat vide (repli sur
//! l'identifiant, anomalie journalisée) ou déjà pris (suffixe numérique).

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Décompose en NFD, retire les marques combinantes, passe en minuscules, remplace tout ce qui
/// n'est pas `[a-z0-9]` par `-`, réduit les séquences de `-` et retire les tirets en tête/queue.
///
/// Ne transcrit pas les caractères qui ne se décomposent pas en ASCII sous NFD (les ligatures
/// comme `Æ`, par exemple) : ils disparaissent simplement, ce qui peut produire une chaîne vide.
/// C'est voulu — c'est au job appelant de journaliser une anomalie et de replier sur l'identifiant
/// (`an_uid`/`wikidata_qid`) plutôt qu'à cette fonction de deviner une transcription.
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_dash = false;

    for c in input
        .to_lowercase()
        .nfd()
        .filter(|c| !is_combining_mark(*c))
    {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c);
        } else {
            pending_dash = true;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn strips_accents() {
        assert_eq!(slugify("Mélenchon"), "melenchon");
    }

    #[test]
    fn turns_spaces_into_dashes() {
        assert_eq!(slugify("Le Pen"), "le-pen");
    }

    #[test]
    fn keeps_an_existing_dash_as_a_single_dash() {
        assert_eq!(slugify("Jean-Luc"), "jean-luc");
    }

    #[test]
    fn turns_apostrophes_into_dashes() {
        assert_eq!(slugify("d'Estaing"), "d-estaing");
    }

    #[test]
    fn strips_accents_and_spaces_together() {
        assert_eq!(slugify("Ó Ríordáin"), "o-riordain");
    }

    /// `Æ` n'est pas une composition canonique lettre + diacritique : NFD ne la décompose pas en
    /// `a` + `e`. Elle disparaît donc silencieusement plutôt que d'être devinée — le repli sur
    /// l'identifiant, côté job, gère ce cas.
    #[test]
    fn a_ligature_that_nfd_does_not_decompose_falls_through_to_empty() {
        assert_eq!(slugify("Æ"), "");
    }

    #[test]
    fn empty_string_stays_empty() {
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn punctuation_only_becomes_empty() {
        assert_eq!(slugify("!!!"), "");
    }

    #[test]
    fn collapses_runs_of_separators_into_a_single_dash() {
        assert_eq!(slugify("Jean  --  Luc"), "jean-luc");
    }

    #[test]
    fn never_starts_or_ends_with_a_dash() {
        assert_eq!(slugify("  Dupont  "), "dupont");
    }
}
