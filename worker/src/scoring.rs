//! Fonctions pures du calcul de score — voir docs/plans/phase-4-partis-scores.md, section « La
//! formule, écrite une fois pour toutes ». Rien ici n'accède à la base : le job
//! `recompute_scores` assemble les entrées (position votée, poids du thème dans le scrutin,
//! confiance de la catégorisation, bipolarité du scrutin) et applique le seuil minimal de
//! contributions (D4.2) ; ces fonctions ne connaissent que les nombres qu'on leur donne.

/// Ce que porte un vote, une fois le non-votant écarté en amont (methodology.md § 3 : un
/// non-votant n'entre dans aucun calcul, donc n'a même pas de représentation ici).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Pour,
    Contre,
    Abstention,
}

/// `apport(p, s, t)` (methodology.md § 6). `position_pour` décrit ce que veut dire voter *pour* ;
/// voter *contre* place à l'opposé — c'est le signe qui s'inverse, jamais `position_pour`
/// lui-même. Une abstention n'a pas de direction (D4.10) : `None`, pas zéro.
pub fn apport(position: Position, position_pour: f64) -> Option<f64> {
    match position {
        Position::Pour => Some(position_pour),
        Position::Contre => Some(-position_pour),
        Position::Abstention => None,
    }
}

/// `poids(p, s, t)` (D4.9). Pas de facteur de type de scrutin (D4.11) : il vaudrait 1 partout.
/// `bipolarite` à 1 annule le poids ; l'appelant ne doit jamais passer un scrutin de bipolarité
/// inconnue ici — il est exclu en amont, pas traité comme 0 (D4.9).
pub fn poids(poids_theme: f64, confiance: f64, bipolarite: f64) -> f64 {
    poids_theme * confiance * (1.0 - bipolarite)
}

/// Une contribution déjà réduite aux deux nombres dont `aggregate` a besoin : `apport` porte
/// `None` pour une abstention (D4.10), `poids` y vaut alors zéro par construction du schéma
/// (`score_contribution_abstention_sans_poids`).
#[derive(Debug, Clone, Copy)]
pub struct Contribution {
    pub apport: Option<f64>,
    pub poids: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aggregate {
    pub score: f64,
    pub incertitude: f64,
    /// Contributions qui pèsent réellement dans la moyenne — pas les abstentions.
    pub contributions: i64,
    pub abstentions: i64,
}

/// `score(p, t)` et `incertitude(p, t)`. `None` seulement quand la moyenne n'existe pas (aucune
/// contribution ne pèse) : c'est le cas dégénéré, pas le seuil éditorial `contributions_min`
/// (D4.2), qui reste la responsabilité de l'appelant.
///
/// `contributions` ne compte que les votes de poids strictement positif (migration 0009 : « compte
/// les votes qui pèsent »). Un vote pour/contre de poids nul (bipolarité 1, D4.9) a un `apport`
/// mais ne pèse rien : il n'est ni une contribution, ni une abstention, il est simplement sans
/// influence — cohérent avec le test qui vérifie qu'un tel vote n'ajoute rien au score, à
/// l'incertitude ni au compteur.
pub fn aggregate(contributions: &[Contribution]) -> Option<Aggregate> {
    let mut weighted_sum = 0.0;
    let mut weight_total = 0.0;
    let mut contributions_count = 0i64;
    let mut abstentions = 0i64;

    for c in contributions {
        match c.apport {
            Some(apport) => {
                weighted_sum += apport * c.poids;
                weight_total += c.poids;
                if c.poids > 0.0 {
                    contributions_count += 1;
                }
            }
            None => abstentions += 1,
        }
    }

    if weight_total <= 0.0 {
        return None;
    }

    let score = weighted_sum / weight_total;
    let variance = contributions
        .iter()
        .filter_map(|c| c.apport.map(|apport| (apport, c.poids)))
        .map(|(apport, poids)| poids * (apport - score).powi(2))
        .sum::<f64>()
        / weight_total;
    let incertitude = variance.sqrt() / (contributions_count as f64).sqrt();

    Some(Aggregate {
        score,
        incertitude,
        contributions: contributions_count,
        abstentions,
    })
}

/// Les votes d'un groupe sur un scrutin, réduits à ce dont `cohesion` a besoin.
#[derive(Debug, Clone, Copy)]
pub struct ScrutinVotesGroupe {
    pub pour: i64,
    pub contre: i64,
}

/// Cohésion d'un groupe sur un thème (D4.4) : part des votes des membres alignés sur la position
/// majoritaire du groupe, scrutin par scrutin, agrégée sur l'ensemble des scrutins du thème où le
/// groupe s'est exprimé. Un scrutin où le groupe n'a émis que des abstentions (`pour + contre ==
/// 0`) ne contribue rien : il n'y a pas de majorité à mesurer sur un camp vide.
pub fn cohesion(scrutins: &[ScrutinVotesGroupe]) -> Option<f64> {
    let mut majoritaires = 0i64;
    let mut total = 0i64;
    for s in scrutins {
        let total_scrutin = s.pour + s.contre;
        if total_scrutin == 0 {
            continue;
        }
        majoritaires += s.pour.max(s.contre);
        total += total_scrutin;
    }
    if total == 0 {
        return None;
    }
    Some(majoritaires as f64 / total as f64)
}
