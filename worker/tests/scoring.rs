//! Tests des fonctions pures de `kyc_worker::scoring` — voir docs/plans/phase-4-partis-scores.md,
//! section « Job `recompute_scores` », la liste de cas « à écrire avant le code ». Aucune base
//! ici : cas construits à la main, résultats attendus écrits à l'avance.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kyc_worker::scoring::{self, Aggregate, Contribution, Position, ScrutinVotesGroupe};

fn assert_close(value: f64, expected: f64) {
    assert!(
        (value - expected).abs() < 1e-9,
        "expected {expected}, got {value}"
    );
}

fn contribution(
    position: Position,
    position_pour: f64,
    poids_theme: f64,
    confiance: f64,
) -> Contribution {
    Contribution {
        apport: scoring::apport(position, position_pour),
        poids: scoring::poids(poids_theme, confiance, 0.0),
    }
}

// --- apport : le signe s'inverse avec le vote, jamais avec position_pour --------------------

#[test]
fn apport_for_pour_equals_position_pour() {
    assert_eq!(scoring::apport(Position::Pour, 0.6), Some(0.6));
    assert_eq!(scoring::apport(Position::Pour, -0.6), Some(-0.6));
}

#[test]
fn apport_for_contre_is_the_opposite_of_position_pour() {
    assert_eq!(scoring::apport(Position::Contre, 0.6), Some(-0.6));
    assert_eq!(scoring::apport(Position::Contre, -0.6), Some(0.6));
}

#[test]
fn apport_for_abstention_is_none() {
    assert_eq!(scoring::apport(Position::Abstention, 0.6), None);
}

// --- poids -------------------------------------------------------------------------------------

#[test]
fn poids_multiplies_the_three_factors() {
    assert_close(scoring::poids(0.5, 0.7, 0.2), 0.5 * 0.7 * 0.8);
}

#[test]
fn a_bipolarite_of_one_nullifies_the_weight() {
    assert_close(scoring::poids(1.0, 1.0, 1.0), 0.0);
}

// --- aggregate -----------------------------------------------------------------------------------

/// Une personne qui vote systématiquement *pour* des textes à `position_pour` négative : le
/// score doit être négatif franc, l'incertitude faible (les apports sont tous égaux).
#[test]
fn systematic_pour_votes_on_negative_texts_give_a_clear_negative_score() {
    let contributions = [
        contribution(Position::Pour, -0.8, 1.0, 1.0),
        contribution(Position::Pour, -0.7, 1.0, 1.0),
        contribution(Position::Pour, -0.9, 1.0, 1.0),
    ];

    let aggregate = scoring::aggregate(&contributions).expect("des contributions qui pèsent");

    assert!(
        aggregate.score < -0.5,
        "score attendu franchement négatif : {}",
        aggregate.score
    );
    assert!(
        aggregate.incertitude < 0.1,
        "apports proches entre eux : incertitude attendue faible, obtenu {}",
        aggregate.incertitude
    );
}

/// La même personne, mais qui vote *contre* les mêmes textes : le score doit s'inverser en signe
/// et garder la même amplitude. C'est le test qui attrape une inversion de signe manquée.
#[test]
fn voting_contre_instead_of_pour_flips_the_sign_of_the_score() {
    let pour_contributions = [
        contribution(Position::Pour, -0.8, 1.0, 1.0),
        contribution(Position::Pour, -0.7, 1.0, 1.0),
        contribution(Position::Pour, -0.9, 1.0, 1.0),
    ];
    let contre_contributions = [
        contribution(Position::Contre, -0.8, 1.0, 1.0),
        contribution(Position::Contre, -0.7, 1.0, 1.0),
        contribution(Position::Contre, -0.9, 1.0, 1.0),
    ];

    let pour = scoring::aggregate(&pour_contributions).expect("des contributions qui pèsent");
    let contre = scoring::aggregate(&contre_contributions).expect("des contributions qui pèsent");

    assert_close(pour.score, -contre.score);
}

/// Des votes contradictoires (apports de signes opposés, poids égaux) : le score doit être
/// proche de zéro et l'incertitude large. Les deux sortent, pas seulement le score.
#[test]
fn contradictory_votes_give_a_score_near_zero_and_a_wide_incertitude() {
    let contributions = [
        contribution(Position::Pour, 0.9, 1.0, 1.0),
        contribution(Position::Contre, 0.9, 1.0, 1.0),
    ];

    let aggregate = scoring::aggregate(&contributions).expect("des contributions qui pèsent");

    assert_close(aggregate.score, 0.0);
    assert!(
        aggregate.incertitude > 0.5,
        "apports opposés : incertitude attendue large, obtenu {}",
        aggregate.incertitude
    );
}

/// Un scrutin à bipolarité 1 a un poids nul (D4.9) : l'ajouter au jeu de contributions ne doit
/// rien changer au score ni à l'incertitude.
#[test]
fn a_contribution_with_zero_weight_has_no_influence() {
    let base = [
        contribution(Position::Pour, 0.6, 1.0, 1.0),
        contribution(Position::Contre, -0.2, 1.0, 1.0),
    ];
    let mut with_bipolar = base.to_vec();
    with_bipolar.push(Contribution {
        apport: scoring::apport(Position::Pour, 0.99),
        poids: scoring::poids(1.0, 1.0, 1.0), // bipolarite = 1 -> poids nul
    });

    let without = scoring::aggregate(&base).unwrap();
    let with = scoring::aggregate(&with_bipolar).unwrap();

    assert_close(with.score, without.score);
    assert_close(with.incertitude, without.incertitude);
    assert_eq!(
        with.contributions, without.contributions,
        "un poids nul ne doit pas compter comme une contribution supplémentaire dans la moyenne"
    );
}

/// Une abstention apparaît dans le jeu de contributions (elle est affichée), a un poids nul,
/// n'entre pas dans la moyenne, et fait avancer le compteur `abstentions` — pas `contributions`.
#[test]
fn an_abstention_is_counted_separately_and_does_not_move_the_score() {
    let without_abstention = [
        contribution(Position::Pour, 0.6, 1.0, 1.0),
        contribution(Position::Pour, 0.6, 1.0, 1.0),
    ];
    let mut with_abstention = without_abstention.to_vec();
    with_abstention.push(Contribution {
        apport: scoring::apport(Position::Abstention, 0.6),
        poids: 0.0,
    });

    let without = scoring::aggregate(&without_abstention).unwrap();
    let with = scoring::aggregate(&with_abstention).unwrap();

    assert_close(with.score, without.score);
    assert_eq!(with.contributions, without.contributions);
    assert_eq!(without.abstentions, 0);
    assert_eq!(with.abstentions, 1);
}

/// Aucune contribution ne pèse (toutes des abstentions, ou un jeu vide) : pas de moyenne
/// possible, `aggregate` rend `None` plutôt qu'une division par zéro déguisée.
#[test]
fn only_abstentions_yields_no_aggregate() {
    let contributions = [
        Contribution {
            apport: scoring::apport(Position::Abstention, 0.5),
            poids: 0.0,
        },
        Contribution {
            apport: scoring::apport(Position::Abstention, -0.2),
            poids: 0.0,
        },
    ];

    assert_eq!(scoring::aggregate(&contributions), None::<Aggregate>);
}

#[test]
fn an_empty_contribution_set_yields_no_aggregate() {
    assert_eq!(scoring::aggregate(&[]), None::<Aggregate>);
}

// --- cohesion --------------------------------------------------------------------------------

/// Un groupe unanime sur chaque scrutin : cohésion maximale.
#[test]
fn a_unanimous_group_has_maximal_cohesion() {
    let scrutins = [
        ScrutinVotesGroupe {
            pour: 40,
            contre: 0,
        },
        ScrutinVotesGroupe {
            pour: 0,
            contre: 38,
        },
    ];

    assert_close(scoring::cohesion(&scrutins).unwrap(), 1.0);
}

/// Un groupe exactement à 50/50 sur chaque scrutin : cohésion minimale (une majorité écrasante
/// et une division à 51 % ne se valent pas, D4.4).
#[test]
fn an_evenly_split_group_has_minimal_cohesion() {
    let scrutins = [ScrutinVotesGroupe {
        pour: 20,
        contre: 20,
    }];

    assert_close(scoring::cohesion(&scrutins).unwrap(), 0.5);
}

/// Cohésion agrégée sur plusieurs scrutins : la part des voix côté majoritaire, toutes voix
/// confondues (pas la moyenne des taux par scrutin).
#[test]
fn cohesion_aggregates_across_several_scrutins_by_vote_count() {
    let scrutins = [
        ScrutinVotesGroupe {
            pour: 90,
            contre: 10,
        }, // majorité 90/100
        ScrutinVotesGroupe {
            pour: 10,
            contre: 10,
        }, // majorité 10/20 (égalité, un seul camp compté)
    ];

    // (90 + 10) / (100 + 20) = 100 / 120
    assert_close(scoring::cohesion(&scrutins).unwrap(), 100.0 / 120.0);
}

/// Un scrutin où le groupe ne s'est exprimé que par abstentions (pour = contre = 0) ne contribue
/// rien : ni au numérateur, ni au dénominateur.
#[test]
fn a_scrutin_with_no_pour_or_contre_votes_is_ignored() {
    let scrutins = [
        ScrutinVotesGroupe {
            pour: 30,
            contre: 0,
        },
        ScrutinVotesGroupe { pour: 0, contre: 0 },
    ];

    assert_close(scoring::cohesion(&scrutins).unwrap(), 1.0);
}

#[test]
fn cohesion_of_an_empty_or_fully_abstaining_set_is_none() {
    assert_eq!(scoring::cohesion(&[]), None);
    assert_eq!(
        scoring::cohesion(&[ScrutinVotesGroupe { pour: 0, contre: 0 }]),
        None
    );
}

// --- desaccord_mesure (F1, docs/plans/phase-4.1-partis-scores.md) ------------------------------

#[test]
fn desaccord_mesure_is_false_off_a_left_right_axis() {
    // institutions-democratie, agriculture, europe (F2) : la mesure ne peut rien arbitrer.
    assert!(!scoring::desaccord_mesure(false, -0.6, 0.3));
}

#[test]
fn desaccord_mesure_is_false_when_the_estimate_is_exactly_zero() {
    assert!(!scoring::desaccord_mesure(true, -0.6, 0.0));
}

#[test]
fn desaccord_mesure_is_true_on_opposite_signs_on_a_left_right_axis() {
    assert!(scoring::desaccord_mesure(true, -0.6, 0.3));
    assert!(scoring::desaccord_mesure(true, 0.6, -0.3));
}

#[test]
fn desaccord_mesure_is_false_on_matching_signs() {
    assert!(!scoring::desaccord_mesure(true, -0.6, -0.3));
    assert!(!scoring::desaccord_mesure(true, 0.6, 0.3));
}

/// Cas défensif, impossible en pratique (un `position_pour` de catégorisation nul est interdit à
/// la saisie) mais le code ne doit pas s'y fier : un produit nul n'est jamais strictement négatif,
/// donc jamais écarté.
#[test]
fn desaccord_mesure_defensively_retains_a_zero_label_position() {
    assert!(!scoring::desaccord_mesure(true, 0.0, 0.5));
}

/// Le cas qui a motivé la phase 4.1 (F1) : trois votes contre sur des textes à `position_pour`
/// négatif (« contrainte réglementaire ») dont la mesure automatique d'axe les classe pourtant du
/// côté positif — un désaccord de signe entre les deux lectures indépendantes. Sans le correctif,
/// ces trois votes contre produiraient mécaniquement un score positif franc (+0.6) : le mauvais
/// pôle, sur le thème le plus identitaire du cas réel qui a motivé cette phase (Mélenchon /
/// environnement). Avec le correctif, les trois contributions sont écartées et l'agrégat rend
/// `None` (« données insuffisantes »), jamais un score inversé.
#[test]
fn melenchon_environnement_case_gives_no_aggregate_not_an_inverted_score() {
    let position_pour = -0.6;
    let estimate_position_pour = 0.3;

    let contributions: Vec<Contribution> = (0..3)
        .map(|_| {
            let apport = scoring::apport(Position::Contre, position_pour);
            let ecartee = scoring::desaccord_mesure(true, position_pour, estimate_position_pour);
            let poids = if ecartee {
                0.0
            } else {
                scoring::poids(1.0, 1.0, 0.0)
            };
            Contribution { apport, poids }
        })
        .collect();

    assert_eq!(scoring::aggregate(&contributions), None::<Aggregate>);
}
