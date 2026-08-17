//! Parsing des acteurs, organes et mandats d'AMO30, et normalisation des chevauchements de
//! mandats de groupe — voir docs/plans/phase-1-ingestion.md, sections « Le référentiel » et
//! « Job ingest_acteurs ».

use chrono::NaiveDate;
use serde_json::Value;

use super::json::{collection, field, scalaire};

fn parse_date(s: Option<&str>) -> Option<NaiveDate> {
    s.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}

pub struct ParsedActeur {
    pub an_uid: String,
    pub civilite: Option<String>,
    pub prenom: Option<String>,
    pub nom: Option<String>,
    pub date_naissance: Option<NaiveDate>,
    pub ville_naissance: Option<String>,
    pub departement_naissance: Option<String>,
    pub pays_naissance: Option<String>,
    pub date_deces: Option<NaiveDate>,
    pub uri_hatvp: Option<String>,
    pub mandats: Vec<ParsedMandat>,
}

pub struct ParsedMandat {
    pub an_uid: String,
    pub organe_uid: Option<String>,
    pub type_organe: Option<String>,
    pub date_debut: Option<NaiveDate>,
    pub date_fin: Option<NaiveDate>,
    pub qualite: Option<String>,
    pub legislature: Option<i16>,
}

pub struct ParsedOrgane {
    pub an_uid: String,
    pub code_type: String,
    pub libelle: String,
    pub libelle_abrege: Option<String>,
    pub libelle_abrev: Option<String>,
    pub legislature: Option<i16>,
    pub date_debut: Option<NaiveDate>,
    pub date_fin: Option<NaiveDate>,
}

/// `payload` est le document `{"acteur": {...}}` tel que reçu, avant tout dépouillement.
pub fn parse_acteur(payload: &Value) -> Option<ParsedActeur> {
    let root = field(payload, "acteur");
    let an_uid = scalaire(field(root, "uid"))?.to_string();

    let etat_civil = field(root, "etatCivil");
    let ident = field(etat_civil, "ident");
    let info_naissance = field(etat_civil, "infoNaissance");

    let mandats = collection(field(field(root, "mandats"), "mandat"))
        .filter_map(parse_mandat)
        .collect();

    Some(ParsedActeur {
        an_uid,
        civilite: scalaire(field(ident, "civ")).map(String::from),
        prenom: scalaire(field(ident, "prenom")).map(String::from),
        nom: scalaire(field(ident, "nom")).map(String::from),
        date_naissance: parse_date(scalaire(field(info_naissance, "dateNais"))),
        ville_naissance: scalaire(field(info_naissance, "villeNais")).map(String::from),
        departement_naissance: scalaire(field(info_naissance, "depNais")).map(String::from),
        pays_naissance: scalaire(field(info_naissance, "paysNais")).map(String::from),
        date_deces: parse_date(scalaire(field(etat_civil, "dateDeces"))),
        uri_hatvp: scalaire(field(root, "uri_hatvp")).map(String::from),
        mandats,
    })
}

fn parse_mandat(m: &Value) -> Option<ParsedMandat> {
    let an_uid = scalaire(field(m, "uid"))?.to_string();
    Some(ParsedMandat {
        an_uid,
        organe_uid: scalaire(field(field(m, "organes"), "organeRef")).map(String::from),
        type_organe: scalaire(field(m, "typeOrgane")).map(String::from),
        date_debut: parse_date(scalaire(field(m, "dateDebut"))),
        date_fin: parse_date(scalaire(field(m, "dateFin"))),
        qualite: scalaire(field(field(m, "infosQualite"), "codeQualite")).map(String::from),
        legislature: scalaire(field(m, "legislature")).and_then(|s| s.parse().ok()),
    })
}

/// `payload` est le document `{"organe": {...}}` tel que reçu.
pub fn parse_organe(payload: &Value) -> Option<ParsedOrgane> {
    let root = field(payload, "organe");
    let an_uid = scalaire(field(root, "uid"))?.to_string();
    let code_type = scalaire(field(root, "codeType"))?.to_string();
    let libelle = scalaire(field(root, "libelle"))?.to_string();
    let vimode = field(root, "viMoDe");

    Some(ParsedOrgane {
        an_uid,
        code_type,
        libelle,
        libelle_abrege: scalaire(field(root, "libelleAbrege")).map(String::from),
        libelle_abrev: scalaire(field(root, "libelleAbrev")).map(String::from),
        legislature: scalaire(field(root, "legislature")).and_then(|s| s.parse().ok()),
        date_debut: parse_date(scalaire(field(vimode, "dateDebut"))),
        date_fin: parse_date(scalaire(field(vimode, "dateFin"))),
    })
}

// --- Normalisation des chevauchements de mandats de groupe ---------------------------------
//
// Voir docs/plans/phase-1-ingestion.md, « Normaliser les mandats de groupe avant insertion » :
// une plage contenue dans une autre est écartée (`mandat_inclus`), deux plages consécutives qui
// partagent leur date de charnière voient la première raccourcie d'un jour (`mandat_charniere`),
// et un chevauchement résiduel entre deux organes différents est signalé (`mandat_chevauchement`)
// mais **conservé** — décision D1.16, docs/plans/phase-1.1-fix.md, F5. Fonction pure, testée
// indépendamment de la base et du job.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MandatGroupe {
    pub an_uid: String,
    pub organe_uid: String,
    pub debut: NaiveDate,
    /// `None` = mandat en cours.
    pub fin: Option<NaiveDate>,
    /// Le pseudo-groupe « non-inscrit » n'est pas une appartenance réelle (methodology.md, § 2) :
    /// l'AN ne le clôture pas toujours proprement quand la personne rejoint un vrai groupe, ce qui
    /// le fait chevaucher son groupe suivant en permanence sans que ce soit une vraie
    /// contradiction. Il est donc exclu du contrôle de chevauchement inter-organes.
    pub is_non_inscrit: bool,
}

fn fin_ou_infini(m: &MandatGroupe) -> NaiveDate {
    m.fin.unwrap_or(NaiveDate::MAX)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalizationEvent {
    /// `an_uid` du mandat écarté parce que sa plage est incluse dans une autre.
    Inclus { an_uid: String },
    /// `an_uid` du mandat dont la fin a été ramenée à la veille pour lever le partage de
    /// charnière avec le mandat suivant.
    Charniere { an_uid: String },
    /// `an_uid` du plus court des deux mandats d'un chevauchement entre organes différents,
    /// signalé à titre indicatif — **les deux mandats sont conservés** (D1.16,
    /// docs/plans/phase-1.1-fix.md, F5), ce n'est plus le mandat écarté.
    Chevauchement { an_uid: String },
}

pub struct NormalizationResult {
    pub retained: Vec<MandatGroupe>,
    pub events: Vec<NormalizationEvent>,
}

pub fn normalize_gp_mandats(mut mandats: Vec<MandatGroupe>) -> NormalizationResult {
    mandats.sort_by(|a, b| {
        a.organe_uid
            .cmp(&b.organe_uid)
            .then(a.debut.cmp(&b.debut))
            .then(fin_ou_infini(a).cmp(&fin_ou_infini(b)))
    });

    let mut events = Vec::new();
    let mut per_organe: Vec<MandatGroupe> = Vec::new();

    let mut start = 0;
    while start < mandats.len() {
        let mut end = start + 1;
        while end < mandats.len() && mandats[end].organe_uid == mandats[start].organe_uid {
            end += 1;
        }
        normalize_same_organe(&mandats[start..end], &mut per_organe, &mut events);
        start = end;
    }

    // Chevauchement résiduel entre organes différents : mesuré en pratique très au-delà du seul
    // cas de 2007 relevé par le spike (voir docs/data-sources.md) — la scission UMP /
    // Rassemblement-UMP de novembre 2012 à elle seule en produit 73, un vrai fait politique et non
    // une anomalie de saisie. Décision D1.16 (docs/plans/phase-1.1-fix.md, F5) : **on conserve les
    // deux mandats et on se contente de signaler chaque paire qui se recouvre encore**, sauf
    // quand l'un des deux est le pseudo-groupe « non-inscrit » — il n'est pas une appartenance
    // réelle (methodology.md, § 2) et l'AN ne le clôture pas toujours à la date où la personne
    // rejoint un vrai groupe, ce qui le ferait sinon chevaucher en permanence sans que ce soit une
    // vraie contradiction. Trancher laquelle des deux plages contradictoires est la « bonne » est
    // une interprétation, pas un fait brut : ça relève de la phase 4, jamais de l'ingestion
    // (CLAUDE.md, « Les données brutes ne sont jamais écrasées par une interprétation »).
    per_organe.sort_by_key(|m| m.debut);
    for i in 0..per_organe.len() {
        for j in (i + 1)..per_organe.len() {
            let (a, b) = (&per_organe[i], &per_organe[j]);
            if a.is_non_inscrit || b.is_non_inscrit {
                continue;
            }
            let overlaps = a.debut <= fin_ou_infini(b) && b.debut <= fin_ou_infini(a);
            if !overlaps {
                continue;
            }
            let a_len = fin_ou_infini(a) - a.debut;
            let b_len = fin_ou_infini(b) - b.debut;
            // Nomme la plage la plus courte des deux dans l'événement, à titre indicatif
            // seulement : les deux mandats sont retenus quoi qu'il en soit.
            let shorter = if b_len < a_len { b } else { a };
            events.push(NormalizationEvent::Chevauchement {
                an_uid: shorter.an_uid.clone(),
            });
        }
    }

    NormalizationResult {
        retained: per_organe,
        events,
    }
}

/// Normalise les mandats d'un seul (personne, organe), déjà triés par date de début puis de fin.
fn normalize_same_organe(
    group: &[MandatGroupe],
    retained: &mut Vec<MandatGroupe>,
    events: &mut Vec<NormalizationEvent>,
) {
    let mut current: Option<MandatGroupe> = None;

    for next in group {
        let Some(mut cur) = current.take() else {
            current = Some(next.clone());
            continue;
        };

        if next.debut > fin_ou_infini(&cur) {
            // Pas de chevauchement : le mandat courant est définitivement retenu.
            retained.push(cur);
            current = Some(next.clone());
            continue;
        }

        if fin_ou_infini(next) <= fin_ou_infini(&cur) {
            // Inclusion : la plage de `next` est contenue dans celle de `cur`, elle est écartée.
            events.push(NormalizationEvent::Inclus {
                an_uid: next.an_uid.clone(),
            });
            current = Some(cur);
            continue;
        }

        // Inclusion inverse : `cur` est contenu dans `next`. Le tri (début croissant, fin
        // croissante) garantit `cur.debut <= next.debut` ; une inclusion de `cur` dans `next`
        // n'est donc possible qu'à égalité stricte des débuts (sinon `next` commencerait après
        // la fin de `cur` sans la contenir). Cas manqué par le premier passage du spike : sans
        // cette branche, ces paires tombent dans le chevauchement résiduel ci-dessous — la bonne
        // plage est tout de même écartée car elle est la plus courte, mais l'anomalie est mal
        // qualifiée (`mandat_chevauchement` au lieu de `mandat_inclus`).
        if cur.debut == next.debut {
            events.push(NormalizationEvent::Inclus {
                an_uid: cur.an_uid.clone(),
            });
            current = Some(next.clone());
            continue;
        }

        if next.debut == fin_ou_infini(&cur) {
            // Charnière : les deux plages partagent leur jour de bascule. On raccourcit `cur`
            // d'un jour et on garde les deux mandats.
            events.push(NormalizationEvent::Charniere {
                an_uid: cur.an_uid.clone(),
            });
            cur.fin = Some(next.debut - chrono::Duration::days(1));
            retained.push(cur);
            current = Some(next.clone());
            continue;
        }

        // Chevauchement partiel résiduel qui n'est ni une inclusion ni une charnière : on écarte
        // le plus court des deux, comme pour le cas résiduel inter-organes.
        let cur_len = fin_ou_infini(&cur) - cur.debut;
        let next_len = fin_ou_infini(next) - next.debut;
        if next_len < cur_len {
            events.push(NormalizationEvent::Chevauchement {
                an_uid: next.an_uid.clone(),
            });
            current = Some(cur);
        } else {
            events.push(NormalizationEvent::Chevauchement {
                an_uid: cur.an_uid.clone(),
            });
            current = Some(next.clone());
        }
    }

    if let Some(cur) = current {
        retained.push(cur);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn mandat(an_uid: &str, organe: &str, debut: &str, fin: Option<&str>) -> MandatGroupe {
        MandatGroupe {
            an_uid: an_uid.to_string(),
            organe_uid: organe.to_string(),
            debut: date(debut),
            fin: fin.map(date),
            is_non_inscrit: false,
        }
    }

    // --- parse_acteur ------------------------------------------------------------------------

    #[test]
    fn parse_acteur_forme_reelle_amo30() {
        // Extrait réel simplifié de json/acteur/PA267551.json.
        let payload = json!({
            "acteur": {
                "uid": {"@xsi:type": "IdActeur_type", "#text": "PA267551"},
                "etatCivil": {
                    "ident": {
                        "civ": "M.",
                        "prenom": "Jacques",
                        "nom": "Lamblin",
                        "trigramme": {"@xsi:nil": "true"}
                    },
                    "infoNaissance": {
                        "dateNais": "1952-08-29",
                        "villeNais": "Nancy",
                        "depNais": "Meurthe-et-Moselle",
                        "paysNais": "France"
                    },
                    "dateDeces": {"@xsi:nil": "true"}
                },
                "uri_hatvp": {"@xsi:nil": "true"},
                "mandats": {
                    "mandat": [
                        {
                            "uid": "PM708140",
                            "typeOrgane": "GP",
                            "dateDebut": "2015-06-02",
                            "dateFin": "2017-06-20",
                            "infosQualite": {"codeQualite": "Membre"},
                            "organes": {"organeRef": "PO707869"}
                        }
                    ]
                }
            }
        });

        let acteur = parse_acteur(&payload).unwrap();
        assert_eq!(acteur.an_uid, "PA267551");
        assert_eq!(acteur.prenom.as_deref(), Some("Jacques"));
        assert_eq!(acteur.nom.as_deref(), Some("Lamblin"));
        assert_eq!(acteur.date_naissance, Some(date("1952-08-29")));
        assert_eq!(acteur.date_deces, None);
        assert_eq!(acteur.uri_hatvp, None);
        assert_eq!(acteur.mandats.len(), 1);
        assert_eq!(acteur.mandats[0].organe_uid.as_deref(), Some("PO707869"));
        assert_eq!(acteur.mandats[0].type_organe.as_deref(), Some("GP"));
    }

    #[test]
    fn parse_acteur_mandat_unique_forme_objet_pas_tableau() {
        let payload = json!({
            "acteur": {
                "uid": "PA1",
                "mandats": {
                    "mandat": {
                        "uid": "PM1",
                        "typeOrgane": "GP",
                        "dateDebut": "2020-01-01",
                        "dateFin": null,
                        "organes": {"organeRef": "PO1"}
                    }
                }
            }
        });
        let acteur = parse_acteur(&payload).unwrap();
        assert_eq!(acteur.mandats.len(), 1);
        assert_eq!(acteur.mandats[0].date_fin, None);
    }

    // --- parse_organe --------------------------------------------------------------------------

    #[test]
    fn parse_organe_uid_chaine_directe() {
        // Forme réelle mesurée sur PO684277 : uid est une chaîne directe, pas un objet à #text.
        let payload = json!({
            "organe": {
                "uid": "PO845401",
                "codeType": "GP",
                "libelle": "Rassemblement National",
                "libelleAbrev": "RN",
                "legislature": "17",
                "viMoDe": {"dateDebut": "2024-07-18", "dateFin": null}
            }
        });
        let organe = parse_organe(&payload).unwrap();
        assert_eq!(organe.an_uid, "PO845401");
        assert_eq!(organe.code_type, "GP");
        assert_eq!(organe.legislature, Some(17));
        assert_eq!(organe.date_fin, None);
    }

    #[test]
    fn parse_organe_sans_date_debut() {
        // Forme réelle mesurée sur un PARPOL (« Le Centre pour la France ») : dateDebut est nil,
        // dateFin renseignée.
        let payload = json!({
            "organe": {
                "uid": "PO685806",
                "codeType": "PARPOL",
                "libelle": "Le Centre pour la France",
                "viMoDe": {"dateDebut": null, "dateFin": "2025-12-03"}
            }
        });
        let organe = parse_organe(&payload).unwrap();
        assert_eq!(organe.date_debut, None);
        assert_eq!(organe.date_fin, Some(date("2025-12-03")));
    }

    // --- normalize_gp_mandats : inclusion --------------------------------------------------

    #[test]
    fn inclusion_ecarte_la_plage_incluse() {
        let mandats = vec![
            mandat("MDT1", "PO1", "2020-01-01", Some("2022-01-01")),
            mandat("MDT2", "PO1", "2020-06-01", Some("2020-08-01")),
        ];
        let result = normalize_gp_mandats(mandats);
        assert_eq!(result.retained.len(), 1);
        assert_eq!(result.retained[0].an_uid, "MDT1");
        assert_eq!(
            result.events,
            vec![NormalizationEvent::Inclus {
                an_uid: "MDT2".to_string()
            }]
        );
    }

    /// Régression : quand deux mandats partagent exactement la même date de début, celui trié en
    /// premier (`cur`) peut être le plus court des deux — l'inclusion est alors « inversée ». Le
    /// tri (début croissant, fin croissante) garantit que ce cas ne peut se produire qu'à égalité
    /// stricte des débuts.
    #[test]
    fn inclusion_inversee_meme_debut_second_plus_long() {
        let mandats = vec![
            mandat("MDT1", "PO1", "2020-01-01", Some("2020-06-01")),
            mandat("MDT2", "PO1", "2020-01-01", Some("2022-01-01")),
        ];
        let result = normalize_gp_mandats(mandats);
        assert_eq!(result.retained.len(), 1);
        assert_eq!(result.retained[0].an_uid, "MDT2");
        assert_eq!(
            result.events,
            vec![NormalizationEvent::Inclus {
                an_uid: "MDT1".to_string()
            }]
        );
    }

    #[test]
    fn inclusion_egale_doublon_exact() {
        // Mesuré par le spike : des doublons exacts (mêmes dates, deux uid de mandat distincts).
        let mandats = vec![
            mandat("MDT1", "PO1", "2020-01-01", Some("2021-01-01")),
            mandat("MDT2", "PO1", "2020-01-01", Some("2021-01-01")),
        ];
        let result = normalize_gp_mandats(mandats);
        assert_eq!(result.retained.len(), 1);
    }

    // --- normalize_gp_mandats : charnière ---------------------------------------------------

    #[test]
    fn charniere_raccourcit_le_premier_d_un_jour() {
        let mandats = vec![
            mandat("MDT1", "PO1", "2018-01-01", Some("2020-06-15")),
            mandat("MDT2", "PO1", "2020-06-15", Some("2022-01-01")),
        ];
        let result = normalize_gp_mandats(mandats);
        assert_eq!(result.retained.len(), 2);
        let premier = result.retained.iter().find(|m| m.an_uid == "MDT1").unwrap();
        assert_eq!(premier.fin, Some(date("2020-06-14")));
        let second = result.retained.iter().find(|m| m.an_uid == "MDT2").unwrap();
        assert_eq!(second.debut, date("2020-06-15"));
        assert_eq!(
            result.events,
            vec![NormalizationEvent::Charniere {
                an_uid: "MDT1".to_string()
            }]
        );
    }

    // --- normalize_gp_mandats : pas de chevauchement ---------------------------------------

    #[test]
    fn mandats_disjoints_sont_tous_retenus() {
        let mandats = vec![
            mandat("MDT1", "PO1", "2017-06-21", Some("2019-01-01")),
            mandat("MDT2", "PO1", "2019-01-02", Some("2022-06-21")),
        ];
        let result = normalize_gp_mandats(mandats);
        assert_eq!(result.retained.len(), 2);
        assert!(result.events.is_empty());
    }

    #[test]
    fn mandat_en_cours_sans_date_fin_est_conserve() {
        let mandats = vec![mandat("MDT1", "PO1", "2024-07-18", None)];
        let result = normalize_gp_mandats(mandats);
        assert_eq!(result.retained.len(), 1);
        assert_eq!(result.retained[0].fin, None);
    }

    // --- normalize_gp_mandats : chevauchement résiduel entre organes différents -----------

    #[test]
    fn chevauchement_entre_organes_differents_est_signale_sans_ecarter() {
        let mandats = vec![
            mandat("MDT1", "PO1", "2007-01-01", Some("2007-12-31")),
            mandat("MDT2", "PO2", "2007-06-01", Some("2007-08-01")),
        ];
        let result = normalize_gp_mandats(mandats);
        assert_eq!(
            result.retained.len(),
            2,
            "les deux mandats sont conservés (D1.16)"
        );
        assert!(result.retained.iter().any(|m| m.an_uid == "MDT1"));
        assert!(result.retained.iter().any(|m| m.an_uid == "MDT2"));
        assert_eq!(
            result.events,
            vec![NormalizationEvent::Chevauchement {
                an_uid: "MDT2".to_string()
            }]
        );
    }

    /// Régression : mesuré sur les données réelles (77 cas dans AMO30, très au-delà du seul cas
    /// de 2007 relevé par le spike), une bascule vers un vrai groupe chevauche presque toujours
    /// le pseudo-groupe « non-inscrit » qui la précède, parce que l'AN ne clôture pas
    /// systématiquement le mandat NI à la date exacte du départ. Ce n'est pas une vraie
    /// contradiction (le NI n'est pas une appartenance, methodology.md § 2) : les deux mandats
    /// doivent être conservés sans déclencher de `Chevauchement`.
    #[test]
    fn chevauchement_avec_un_pseudo_groupe_non_inscrit_est_ignore() {
        let mut ni = mandat("MDT_NI", "PO_NI", "2024-07-08", None);
        ni.is_non_inscrit = true;
        let reel = mandat("MDT_REEL", "PO_REEL", "2024-07-19", Some("2024-09-20"));

        let mandats = vec![ni, reel];
        let result = normalize_gp_mandats(mandats);

        assert_eq!(result.retained.len(), 2);
        assert!(result.events.is_empty());
    }

    /// Le vrai chevauchement mesuré à grande échelle : la scission UMP / Rassemblement-UMP de
    /// novembre 2012 à janvier 2013, où 73 député·es sont recensé·es simultanément dans les deux
    /// groupes. Aucun des deux organes n'est un pseudo-groupe : le chevauchement est réel, la base
    /// l'accepte et le signale (D1.16), elle ne tranche plus laquelle des deux plages est vraie.
    #[test]
    fn chevauchement_reel_entre_deux_vrais_groupes_est_signale_sans_ecarter() {
        let mandats = vec![
            mandat("MDT_UMP", "PO_UMP", "2012-06-26", Some("2015-06-01")),
            mandat("MDT_RUMP", "PO_RUMP", "2012-11-27", Some("2013-01-15")),
        ];
        let result = normalize_gp_mandats(mandats);

        assert_eq!(
            result.retained.len(),
            2,
            "les deux mandats coexistent (D1.16)"
        );
        assert!(result.retained.iter().any(|m| m.an_uid == "MDT_UMP"));
        assert!(result.retained.iter().any(|m| m.an_uid == "MDT_RUMP"));
        assert_eq!(
            result.events,
            vec![NormalizationEvent::Chevauchement {
                an_uid: "MDT_RUMP".to_string()
            }]
        );
    }
}
