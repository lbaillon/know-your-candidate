//! Tests des fonctions pures de `kyc_worker::axis` — voir docs/plans/phase-3-categorisation.md,
//! section « Job `label_scrutins_heuristic` ». Aucune base ici : ce sont des cas construits à la
//! main, résultats attendus écrits à l'avance, comme demandé par le plan.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kyc_worker::axis::{self, GroupeVote, Refus};

fn covered_group(pour: i64, contre: i64, coordonnee: f64) -> GroupeVote {
    GroupeVote {
        pour,
        contre,
        coordonnee: Some(coordonnee),
    }
}

fn uncovered_group(pour: i64, contre: i64) -> GroupeVote {
    GroupeVote {
        pour,
        contre,
        coordonnee: None,
    }
}

fn assert_close(value: f64, expected: f64) {
    assert!(
        (value - expected).abs() < 1e-9,
        "expected {expected}, got {value}"
    );
}

/// Polarisation parfaite : la gauche vote intégralement contre, la droite intégralement pour.
/// L'axe explique le scrutin sans reste : séparation maximale.
#[test]
fn perfect_polarization_gives_maximal_separation() {
    let groupes = [
        covered_group(0, 120, -0.8), // gauche, contre
        covered_group(0, 90, -0.4),  // centre-gauche, contre
        covered_group(110, 0, 0.5),  // droite, pour
        covered_group(60, 0, 0.9),   // extrême droite, pour
    ];

    let estimation = axis::estimate(&groupes).expect("scrutin exploitable");

    assert_close(estimation.separation, 1.0);
    assert!(
        estimation.position_pour > 0.0,
        "le camp pour est à droite du camp contre : position_pour = {}",
        estimation.position_pour
    );
    assert_close(estimation.couverture, 1.0);
}

/// Le cas qui justifie la colonne `separation` (D3.9, docs/plans/phase-3-categorisation.md) :
/// scrutin 17/2653, RN et UDR (extrême droite) pour, tout le reste contre, du centre à la droite
/// classique en passant par la gauche. La moyenne du camp contre est proche du centre par simple
/// annulation des extrêmes — c'est justement ce que `separation` doit rendre lisible plutôt que
/// laisser `position_pour` seul suggérer un vote équilibré.
///
/// Correction actée en session d'implémentation (voir le commit) : le plan attendait une
/// « séparation médiocre » pour ce cas. L'algorithme tel qu'arbitré (recherche du meilleur seuil
/// parmi tous les seuils possibles, D3.9) trouve au contraire un seuil qui isole exactement
/// RN + UDR du reste et classe donc la quasi-totalité des votants correctement : la séparation est
/// **élevée**, pas médiocre. C'est le comportement correct de la mesure telle que spécifiée — elle
/// dit bien qu'ici, l'axe *sépare* effectivement le vote, seule sa position moyenne muette sur le
/// camp contre pouvait laisser croire le contraire.
#[test]
fn scrutin_17_2653_isolated_rn_udr_gives_high_separation_not_poor() {
    let groupes = [
        covered_group(122, 0, 1.0), // RN
        covered_group(16, 0, 0.85), // UDR
        covered_group(0, 70, -1.0), // LFI-NFP
        covered_group(0, 60, -0.5), // SOC
        covered_group(0, 50, 0.3),  // EPR
        covered_group(0, 51, 0.6),  // LR/DR
    ];

    let estimation = axis::estimate(&groupes).expect("scrutin exploitable");

    assert!(
        estimation.position_pour > 0.0,
        "la direction doit rester juste : position_pour = {}",
        estimation.position_pour
    );
    assert_close(estimation.separation, 1.0);
}

/// Un groupe qui se divise en interne (une partie pour, une partie contre) dégrade la séparation
/// sans l'annuler : les deux groupes homogènes restent parfaitement classés, seul le groupe divisé
/// contribue des votants mal classés quel que soit le seuil choisi.
#[test]
fn a_split_group_degrades_separation_without_cancelling_it() {
    let groupes = [
        covered_group(0, 100, -1.0), // toujours contre
        covered_group(100, 0, 1.0),  // toujours pour
        covered_group(30, 30, 0.0),  // partagé
    ];

    let estimation = axis::estimate(&groupes).expect("scrutin exploitable");

    // À n'importe lequel des deux seuils possibles, les deux groupes homogènes (100 votants
    // chacun) sont classés sans erreur ; le groupe partagé (60 votants, 30/30) tombe toujours du
    // côté majoritaire de son seuil, qui n'en récupère que la moitié (30) et perd l'autre moitié.
    // a = (100 + 100 + 30) / 260 = 230 / 260, separation = 2a - 1.
    let a = 230.0 / 260.0;
    assert_close(estimation.separation, 2.0 * a - 1.0);
    assert!(estimation.separation < 1.0);
    assert!(estimation.separation > 0.5);
}

/// Un groupe absent de l'ancrage compte dans `couverture` mais n'entre dans aucun autre calcul :
/// ni `position_pour`, ni `separation` ne changent selon ses votes.
#[test]
fn a_group_absent_from_the_anchor_only_affects_coverage() {
    let without_uncovered_group = [covered_group(50, 40, -0.5), covered_group(60, 45, 0.5)];
    let with_heavy_uncovered_group = [
        covered_group(50, 40, -0.5),
        covered_group(60, 45, 0.5),
        uncovered_group(1000, 1000), // pèserait énormément s'il était pris en compte par erreur
    ];

    let without = axis::estimate(&without_uncovered_group).expect("scrutin exploitable");
    let total_covered: i64 = 50 + 40 + 60 + 45;
    let total_with_uncovered = total_covered + 2000;
    let expected_couverture = total_covered as f64 / total_with_uncovered as f64;

    // La couverture tombe sous 90 % dès qu'un groupe hors ancrage pèse ce lourd : le refus est le
    // signal attendu, pas un calcul silencieusement faux.
    assert!(expected_couverture < axis::COUVERTURE_MIN);
    assert_eq!(
        axis::estimate(&with_heavy_uncovered_group),
        Err(Refus::AncrageInsuffisant)
    );

    // À couverture suffisante en revanche, la présence d'un groupe hors ancrage ne doit rien
    // changer aux autres nombres.
    let with_light_uncovered_group = [
        covered_group(50, 40, -0.5),
        covered_group(60, 45, 0.5),
        uncovered_group(3, 2),
    ];
    let with_light =
        axis::estimate(&with_light_uncovered_group).expect("couverture encore suffisante");
    assert_close(with_light.position_pour, without.position_pour);
    assert_close(with_light.separation, without.separation);
    assert_ne!(with_light.couverture, without.couverture);
}

/// Camp vide : personne, parmi les groupes couverts, n'a voté contre. Aucune estimation ne peut
/// situer une opposition qui n'existe pas.
#[test]
fn an_empty_camp_is_refused_as_unanimous() {
    let groupes = [covered_group(30, 0, -0.5), covered_group(40, 0, 0.5)];
    assert_eq!(axis::estimate(&groupes), Err(Refus::ScrutinUnanime));
}

/// Moins de vingt votants couverts, même à 100 % de couverture : la mesure est refusée pour
/// effectif insuffisant.
#[test]
fn too_few_covered_voters_is_refused() {
    let groupes = [covered_group(5, 4, -0.5), covered_group(6, 3, 0.5)];
    assert_eq!(axis::estimate(&groupes), Err(Refus::TropPeuDeVotants));
}

/// Coordonnées toutes identiques : aucun seuil ne peut séparer quoi que ce soit. Séparation nulle,
/// pas de division par zéro.
#[test]
fn identical_coordinates_give_zero_separation() {
    let groupes = [
        covered_group(30, 20, 0.2),
        covered_group(15, 25, 0.2),
        covered_group(10, 5, 0.2),
    ];

    let estimation = axis::estimate(&groupes).expect("scrutin exploitable");

    assert_close(estimation.separation, 0.0);
}

/// D4.9 : une opposition venue d'un seul bord (le cas normal, gauche OU droite) ne situe pas moins
/// bien la personne qu'un vote classique — bipolarité nulle.
#[test]
fn opposition_from_a_single_side_gives_zero_bipolarite() {
    let groupes = [
        covered_group(200, 0, 0.0),  // camp pour, centré en 0.0
        covered_group(0, 100, -0.5), // camp contre, entièrement à gauche de mu_plus
    ];

    let estimation = axis::estimate(&groupes).expect("scrutin exploitable");

    assert_close(estimation.bipolarite, 0.0);
}

/// Le cas budgétaire de phase 3 (F9, phase-3.0-feedback.md) : un texte porté par le centre, rejeté
/// à parts égales par les deux extrêmes. La bipolarité doit valoir 1 — ce scrutin ne situe
/// personne, quel que soit son `position_pour`.
#[test]
fn opposition_evenly_split_across_both_extremes_gives_maximal_bipolarite() {
    let groupes = [
        covered_group(200, 0, 0.0), // majorité centrale, pour
        covered_group(0, 50, -1.0), // extrême gauche, contre
        covered_group(0, 50, 1.0),  // extrême droite, contre
    ];

    let estimation = axis::estimate(&groupes).expect("scrutin exploitable");

    assert_close(estimation.bipolarite, 1.0);
}

/// Une opposition majoritairement mais pas exclusivement d'un bord donne une bipolarité
/// intermédiaire, proportionnelle au plus petit des deux camps.
#[test]
fn a_lopsided_two_sided_opposition_gives_intermediate_bipolarite() {
    let groupes = [
        covered_group(200, 0, 0.0), // camp pour, centré en 0.0
        covered_group(0, 80, -0.5), // contre, à gauche
        covered_group(0, 20, 0.5),  // contre, à droite (minoritaire)
    ];

    let estimation = axis::estimate(&groupes).expect("scrutin exploitable");

    // g = 80/100, d = 20/100 -> bipolarite = 2 * min(0.8, 0.2) = 0.4
    assert_close(estimation.bipolarite, 0.4);
}

/// Un votant « contre » situé exactement à `mu_plus` ne compte d'aucun côté : ni gauche, ni droite.
#[test]
fn a_contre_vote_exactly_at_mu_plus_counts_on_neither_side() {
    let groupes = [
        covered_group(100, 0, 0.0), // camp pour, mu_plus = 0.0
        covered_group(0, 50, 0.0),  // contre, exactement à mu_plus
        covered_group(0, 50, -1.0), // contre, à gauche
    ];

    let estimation = axis::estimate(&groupes).expect("scrutin exploitable");

    // contre_total = 100 ; gauche = 50 (le groupe à -1.0), droite = 0, le groupe à 0.0 n'est
    // compté nulle part -> g = 0.5, d = 0.0 -> bipolarite = 0.0
    assert_close(estimation.bipolarite, 0.0);
}
