//! Les deux primitives de normalisation du JSON de l'Assemblée nationale — voir
//! docs/plans/phase-1-ingestion.md, section « Les deux primitives de parsing ». Le JSON est un XML
//! converti mécaniquement : tout accès à un champ scalaire ou à une collection doit passer par
//! l'une des deux fonctions ci-dessous, jamais par un accès direct au `serde_json::Value`.

use serde_json::Value;

/// Sentinelle utilisée pour les champs absents d'un objet, afin que `field` puisse renvoyer une
/// référence sans avoir à cloner. `&Value::Null` est un constructeur d'unité, promu `'static` par
/// le compilateur : cette référence est valide aussi longtemps que nécessaire.
const NULL: Value = Value::Null;

/// Accès à un champ d'objet qui peut être absent : renvoie `Value::Null` plutôt que de forcer
/// chaque appelant à gérer un `Option<&Value>` en plus du `Option<&str>` de `scalaire`.
pub fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.as_object().and_then(|obj| obj.get(key)).unwrap_or(&NULL)
}

/// Normalise un scalaire du JSON de l'AN : une valeur est soit une chaîne, soit un objet à
/// `#text` (avec ou sans `@xsi:type`), soit un objet `{"@xsi:nil": "true"}` qui vaut `None`, soit
/// un JSON `null` littéral (fréquent hors des champs typés XML, ex. `"contres": null`).
pub fn scalaire(v: &Value) -> Option<&str> {
    match v {
        Value::String(s) => Some(s.as_str()),
        Value::Null => None,
        Value::Object(obj) => {
            if obj.get("@xsi:nil").and_then(Value::as_str) == Some("true") {
                return None;
            }
            obj.get("#text").and_then(Value::as_str)
        }
        _ => None,
    }
}

/// Itérateur renvoyé par `collection` : un type concret unique qui unifie les trois formes
/// possibles, pour pouvoir être renvoyé sans allocation (`Box<dyn Iterator>`).
pub enum CollectionIter<'a> {
    Empty,
    Single(std::iter::Once<&'a Value>),
    Many(std::slice::Iter<'a, Value>),
}

impl<'a> Iterator for CollectionIter<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<&'a Value> {
        match self {
            CollectionIter::Empty => None,
            CollectionIter::Single(iter) => iter.next(),
            CollectionIter::Many(iter) => iter.next(),
        }
    }
}

/// Normalise une collection du JSON de l'AN : publiée comme un objet quand elle a un seul
/// élément, comme un tableau au-delà, absente (`null` ou clé manquante déjà résolue en
/// `Value::Null` par `field`) quand elle est vide.
pub fn collection(v: &Value) -> CollectionIter<'_> {
    match v {
        Value::Null => CollectionIter::Empty,
        Value::Array(items) => CollectionIter::Many(items.iter()),
        other => CollectionIter::Single(std::iter::once(other)),
    }
}

/// `parDelegation` et les autres booléens de la source sont des chaînes `"true"` / `"false"`,
/// jamais un booléen JSON natif.
pub fn booleen(v: &Value) -> bool {
    scalaire(v) == Some("true")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- scalaire : les trois formes -----------------------------------------------------

    #[test]
    fn scalaire_chaine_directe() {
        let v = json!("PA606171");
        assert_eq!(scalaire(&v), Some("PA606171"));
    }

    #[test]
    fn scalaire_objet_a_text() {
        // Forme réelle de acteur.uid dans AMO30.
        let v = json!({
            "@xmlns:xsi": "http://www.w3.org/2001/XMLSchema-instance",
            "@xsi:type": "IdActeur_type",
            "#text": "PA267551"
        });
        assert_eq!(scalaire(&v), Some("PA267551"));
    }

    #[test]
    fn scalaire_objet_nil_vaut_none() {
        // Forme réelle de acteur.etatCivil.ident.trigramme dans AMO30.
        let v = json!({
            "@xmlns:xsi": "http://www.w3.org/2001/XMLSchema-instance",
            "@xsi:nil": "true"
        });
        assert_eq!(scalaire(&v), None);
    }

    #[test]
    fn scalaire_null_litteral_vaut_none() {
        // Forme réelle de miseAuPoint.contres dans les scrutins.
        let v = Value::Null;
        assert_eq!(scalaire(&v), None);
    }

    #[test]
    fn scalaire_via_field_sur_cle_absente() {
        let obj = json!({"autre": "valeur"});
        assert_eq!(scalaire(field(&obj, "manquant")), None);
    }

    // --- collection : les trois formes ---------------------------------------------------

    #[test]
    fn collection_absente() {
        let v = Value::Null;
        assert_eq!(collection(&v).count(), 0);
    }

    #[test]
    fn collection_objet_singleton() {
        // Forme réelle mesurée : VTANR5L17V2657, groupe.vote.decompteNominatif.pours.votant à un
        // seul élément.
        let v = json!({"acteurRef": "PA569", "mandatRef": "PM843512", "parDelegation": "false"});
        let items: Vec<_> = collection(&v).collect();
        assert_eq!(items.len(), 1);
        assert_eq!(scalaire(field(items[0], "acteurRef")), Some("PA569"));
    }

    #[test]
    fn collection_tableau() {
        let v = json!([
            {"acteurRef": "PA1"},
            {"acteurRef": "PA2"},
            {"acteurRef": "PA3"}
        ]);
        let items: Vec<_> = collection(&v).collect();
        assert_eq!(items.len(), 3);
        assert_eq!(scalaire(field(items[2], "acteurRef")), Some("PA3"));
    }

    // --- booléen ---------------------------------------------------------------------------

    #[test]
    fn booleen_chaine_true() {
        assert!(booleen(&json!("true")));
    }

    #[test]
    fn booleen_chaine_false() {
        assert!(!booleen(&json!("false")));
    }

    #[test]
    fn booleen_absent_vaut_false() {
        assert!(!booleen(&Value::Null));
    }
}
