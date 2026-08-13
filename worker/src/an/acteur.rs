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
// et un chevauchement résiduel entre deux organes différents fait tomber la plage la plus courte
// (`mandat_chevauchement`). Fonction pure, testée indépendamment de la base et du job.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MandatGroupe {
    pub an_uid: String,
    pub organe_uid: String,
    pub debut: NaiveDate,
    /// `None` = mandat en cours.
    pub fin: Option<NaiveDate>,
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
    /// `an_uid` du mandat écarté parce qu'il chevauche un autre organe et est le plus court des
    /// deux.
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

    // Chevauchement résiduel entre organes différents (cas mesuré : 1, hors corpus L15-L17). On
    // retire la plage la plus courte de chaque paire qui se chevauche encore.
    per_organe.sort_by_key(|m| m.debut);
    let mut retained: Vec<MandatGroupe> = Vec::new();
    'outer: for candidate in per_organe {
        for kept in &retained {
            let overlaps =
                candidate.debut <= fin_ou_infini(kept) && kept.debut <= fin_ou_infini(&candidate);
            if !overlaps {
                continue;
            }
            let candidate_len = fin_ou_infini(&candidate) - candidate.debut;
            let kept_len = fin_ou_infini(kept) - kept.debut;
            if candidate_len < kept_len {
                events.push(NormalizationEvent::Chevauchement {
                    an_uid: candidate.an_uid.clone(),
                });
                continue 'outer;
            }
            // Le candidat est plus long (ou égal) : il remplace celui déjà retenu.
            events.push(NormalizationEvent::Chevauchement {
                an_uid: kept.an_uid.clone(),
            });
            let index = retained
                .iter()
                .position(|m| m.an_uid == kept.an_uid)
                .expect("kept vient de retained");
            retained.remove(index);
            retained.push(candidate);
            continue 'outer;
        }
        retained.push(candidate);
    }

    NormalizationResult { retained, events }
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
#[allow(clippy::unwrap_used)]
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
    fn chevauchement_entre_organes_differents_ecarte_le_plus_court() {
        let mandats = vec![
            mandat("MDT1", "PO1", "2007-01-01", Some("2007-12-31")),
            mandat("MDT2", "PO2", "2007-06-01", Some("2007-08-01")),
        ];
        let result = normalize_gp_mandats(mandats);
        assert_eq!(result.retained.len(), 1);
        assert_eq!(result.retained[0].an_uid, "MDT1");
        assert_eq!(
            result.events,
            vec![NormalizationEvent::Chevauchement {
                an_uid: "MDT2".to_string()
            }]
        );
    }
}
