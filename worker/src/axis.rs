//! Fonctions pures du calcul d'ancrage gauche-droite — voir docs/plans/phase-3-categorisation.md,
//! section « Job `label_scrutins_heuristic` ». Rien ici n'accède à la base : c'est ce qui rend ces
//! fonctions testables seules, sur des cas construits à la main plutôt que par un jeu de fixtures.
//!
//! Le calcul travaille en `f64`. Les colonnes de destination sont des `numeric(4,3)` (D3.13), mais
//! cette exigence protège des valeurs *saisies* (la somme des poids d'une catégorisation doit valoir
//! exactement 1, et un aller-retour d'export doit être identique au caractère près) — elle ne
//! s'applique pas à une mesure *calculée* une seule fois et jamais réimportée. PostgreSQL arrondit
//! lui-même le `f64` à trois décimales à l'écriture ; voir le job `label_scrutins_heuristic` pour le
//! détail du cast SQL qui évite d'ajouter une dépendance de calcul décimal au worker.

/// Un groupe ayant voté sur un scrutin donné.
#[derive(Debug, Clone, Copy)]
pub struct GroupeVote {
    pub pour: i64,
    pub contre: i64,
    /// `None` si ce groupe est absent de l'ancrage courant (`group_axis_entry`).
    pub coordonnee: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimation {
    pub position_pour: f64,
    pub separation: f64,
    pub couverture: f64,
    pub votants_couverts: i64,
    /// D4.9, docs/plans/phase-4-partis-scores.md : part du camp « contre » située de part et
    /// d'autre du camp « pour ». 0 quand l'opposition est d'un seul bord (le cas normal), 1 quand
    /// elle est également répartie des deux côtés — signe qu'un budget ou un texte de compromis
    /// est rejeté à la fois par la gauche et par la droite, et qu'il ne situe donc personne.
    pub bipolarite: f64,
}

/// Les trois raisons pour lesquelles aucune estimation n'est écrite (D3.9). Ce sont des compteurs
/// du run, pas des échecs : `label_scrutins_heuristic` les journalise et continue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refus {
    /// `couverture < COUVERTURE_MIN` : trop du scrutin échappe à l'ancrage pour que la mesure ait
    /// un sens.
    AncrageInsuffisant,
    /// Moins de `VOTANTS_COUVERTS_MIN` votants parmi les groupes couverts par l'ancrage.
    TropPeuDeVotants,
    /// Un des deux camps (pour/contre), parmi les groupes couverts, est vide : un scrutin sans
    /// opposition ne situe personne.
    ScrutinUnanime,
}

pub const COUVERTURE_MIN: f64 = 0.90;
pub const VOTANTS_COUVERTS_MIN: i64 = 20;

/// Calcule la mesure `group_alignment` pour un scrutin, ou explique pourquoi elle est refusée.
///
/// Les abstentions et les non-votants n'entrent dans aucun calcul (methodology.md § 3) : ils ne
/// figurent pas dans `GroupeVote`, qui ne porte que `pour` et `contre`.
pub fn estimate(groupes: &[GroupeVote]) -> Result<Estimation, Refus> {
    let total: i64 = groupes.iter().map(|g| g.pour + g.contre).sum();
    let covered: Vec<&GroupeVote> = groupes.iter().filter(|g| g.coordonnee.is_some()).collect();
    let votants_couverts: i64 = covered.iter().map(|g| g.pour + g.contre).sum();

    let couverture = if total == 0 {
        0.0
    } else {
        votants_couverts as f64 / total as f64
    };
    if couverture < COUVERTURE_MIN {
        return Err(Refus::AncrageInsuffisant);
    }
    if votants_couverts < VOTANTS_COUVERTS_MIN {
        return Err(Refus::TropPeuDeVotants);
    }

    let pour_total: i64 = covered.iter().map(|g| g.pour).sum();
    let contre_total: i64 = covered.iter().map(|g| g.contre).sum();
    if pour_total == 0 || contre_total == 0 {
        return Err(Refus::ScrutinUnanime);
    }

    // `covered` ne contient que des `Some` par construction (filtré ci-dessus) : on le convertit
    // une fois en un tableau de coordonnées certaines, ce qui évite tout déballage risqué plus bas.
    let covered: Vec<(i64, i64, f64)> = covered
        .into_iter()
        .filter_map(|g| g.coordonnee.map(|x| (g.pour, g.contre, x)))
        .collect();

    let mu_plus = covered
        .iter()
        .map(|&(pour, _, x)| pour as f64 * x)
        .sum::<f64>()
        / pour_total as f64;
    let mu_minus = covered
        .iter()
        .map(|&(_, contre, x)| contre as f64 * x)
        .sum::<f64>()
        / contre_total as f64;
    let position_pour = ((mu_plus - mu_minus) / 2.0).clamp(-1.0, 1.0);

    Ok(Estimation {
        position_pour,
        separation: separation(&covered, votants_couverts),
        couverture,
        votants_couverts,
        bipolarite: bipolarite(&covered, mu_plus, contre_total),
    })
}

/// `2 × min(g, d)` (D4.9) où `g` et `d` sont les parts du camp « contre » situées strictement à
/// gauche, respectivement à droite, de `mu_plus` — la position moyenne du camp « pour ». Un
/// votant « contre » exactement sur `mu_plus` ne compte d'aucun côté : cas limite qui n'affecte
/// ni le sens ni les bornes de la mesure.
fn bipolarite(covered: &[(i64, i64, f64)], mu_plus: f64, contre_total: i64) -> f64 {
    let mut gauche = 0i64;
    let mut droite = 0i64;
    for &(_, contre, x) in covered {
        match x.partial_cmp(&mu_plus) {
            Some(std::cmp::Ordering::Less) => gauche += contre,
            Some(std::cmp::Ordering::Greater) => droite += contre,
            _ => {}
        }
    }
    let g = gauche as f64 / contre_total as f64;
    let d = droite as f64 / contre_total as f64;
    2.0 * g.min(d)
}

/// Meilleure séparation obtenue en essayant, pour chaque seuil pris au milieu de deux coordonnées
/// consécutives distinctes, de classer chaque groupe couvert du côté « à droite » ou « à gauche »
/// de ce seuil. « Dans les deux orientations » (plan) revient à prendre, de chaque côté, le plus
/// grand des deux comptes (pour ou contre) : chercher explicitement l'autre orientation donnerait
/// le complément et jamais un meilleur score, la meilleure des deux est donc déjà ce maximum.
///
/// Un seuil qui isole un petit camp homogène (voir le commentaire sur le scrutin 17/2653 dans
/// `worker/tests/axis.rs`) peut légitimement obtenir une séparation élevée : la fonction cherche le
/// meilleur seuil, pas un seuil « au centre ». C'est une propriété du calcul tel qu'arbitré, pas un
/// biais de cette implémentation.
fn separation(covered: &[(i64, i64, f64)], total_votants: i64) -> f64 {
    let mut coordonnees: Vec<f64> = covered.iter().map(|&(_, _, x)| x).collect();
    coordonnees.sort_by(f64::total_cmp);
    coordonnees.dedup();

    // Une seule coordonnée distincte : aucun seuil ne sépare rien, la séparation est nulle par
    // définition plutôt que par une division par zéro évitée de justesse.
    if coordonnees.len() < 2 {
        return 0.0;
    }

    let best_rate = coordonnees
        .windows(2)
        .map(|pair| {
            let threshold = (pair[0] + pair[1]) / 2.0;
            let (mut pour_left, mut contre_left) = (0i64, 0i64);
            let (mut pour_right, mut contre_right) = (0i64, 0i64);
            for &(pour, contre, x) in covered {
                if x <= threshold {
                    pour_left += pour;
                    contre_left += contre;
                } else {
                    pour_right += pour;
                    contre_right += contre;
                }
            }
            let correct = pour_left.max(contre_left) + pour_right.max(contre_right);
            correct as f64 / total_votants as f64
        })
        .fold(0.5_f64, f64::max);

    (2.0 * best_rate - 1.0).max(0.0)
}
