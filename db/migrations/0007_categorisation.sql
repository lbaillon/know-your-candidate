-- Phase 3 — catégorisation : thèmes et axes, ancrage gauche-droite mesuré, catégorisations
-- humaines historisées, comptes d'administration, imports.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md).

-- 1. Thèmes et axes ------------------------------------------------------------------------------
--
-- L'axe vit dans `theme` : la relation est 1:1 (methodology.md § 4), une table séparée n'ajouterait
-- qu'une jointure (D3.12). Le thème `autre` (D3.5) est le seul sans axe — ses deux libellés de pôle
-- sont NULL, et une catégorisation qui le porte n'a pas de position.
--
-- Convention de signe, à respecter dans le seed : le pôle **négatif** est celui qui se situe du côté
-- gauche de l'ancrage des nuances. Ce n'est pas un jugement, c'est ce qui rend le signe de
-- `scrutin_axis_estimate.position_pour` comparable à celui d'une catégorisation humaine — sans quoi
-- le contrôle croisé de la section « Fini quand » ne veut rien dire.

CREATE TABLE theme (
    id                   smallint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    slug                 text NOT NULL UNIQUE,
    libelle              text NOT NULL,
    description          text NOT NULL,
    libelle_pole_negatif text,
    libelle_pole_positif text,
    rang                 smallint NOT NULL,
    actif                boolean NOT NULL DEFAULT true,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT theme_axe_complet_ou_absent
        CHECK ((libelle_pole_negatif IS NULL) = (libelle_pole_positif IS NULL)),
    CONSTRAINT theme_slug_non_vide CHECK (btrim(slug) <> '')
);

-- 2. Le corpus est un paramètre, pas une constante ------------------------------------------------
--
-- methodology.md § 2 fixe 50 % « pour la v1 » en disant explicitement que le seuil pourra être
-- abaissé. Mesuré : 988 scrutins à 50 %, 4 318 à 30 %. Déplacer ce chiffre est une décision de
-- charge de travail humain, elle doit être visible et datée, pas cachée dans un WHERE.

CREATE TABLE corpus_parametre (
    id                boolean PRIMARY KEY DEFAULT true CHECK (id),
    participation_min numeric NOT NULL DEFAULT 0.50
                      CHECK (participation_min > 0 AND participation_min <= 1),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    updated_by        text
);

INSERT INTO corpus_parametre (id) VALUES (true);

-- 3. Ancrage gauche-droite des groupes ------------------------------------------------------------
--
-- Une version d'ancrage est IMMUABLE : `content_hash` permet au job de refuser de recharger la même
-- version avec un contenu différent. Changer une coordonnée impose donc de changer la version, et
-- l'explication d'une estimation calculée hier ne peut pas se mettre à mentir en silence.

CREATE TABLE group_axis (
    version        text PRIMARY KEY,
    description    text NOT NULL,
    grille_version text NOT NULL,
    grille_date    date NOT NULL,
    source_url     text NOT NULL,
    content_hash   text NOT NULL,
    is_current     boolean NOT NULL DEFAULT false,
    loaded_at      timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX group_axis_courant_idx ON group_axis (is_current) WHERE is_current;

CREATE TABLE group_axis_entry (
    axis_version text NOT NULL REFERENCES group_axis (version) ON DELETE CASCADE,
    organe_id    bigint NOT NULL REFERENCES organe (id),
    nuance_code  text NOT NULL,
    bloc         text NOT NULL,
    coordonnee   numeric(4,3) NOT NULL CHECK (coordonnee BETWEEN -1 AND 1),
    note         text,
    PRIMARY KEY (axis_version, organe_id)
);

-- 4. La mesure, séparée de l'interprétation (D3.7) -------------------------------------------------
--
-- `position_pour` se lit « voter *pour* ce scrutin situe à cette position, relativement à celles et
-- ceux qui ont voté contre ». `separation` dit à quel point l'axe sépare réellement les deux camps :
-- sans elle, le scrutin 17/2653 (RN + UDR pour, tout le reste contre) rendrait un chiffre juste et
-- muet sur sa propre fragilité.
--
-- La clé étrangère vers `group_axis` sera à relâcher le jour où `principal_axis` arrivera (D3.10) :
-- son axe n'est pas ancré sur un fichier de groupes. C'est une migration d'une ligne, et en attendant
-- l'intégrité référentielle a une valeur.

CREATE TABLE scrutin_axis_estimate (
    scrutin_id       bigint NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    strategy         text   NOT NULL,
    axis_version     text   NOT NULL REFERENCES group_axis (version) ON DELETE CASCADE,
    position_pour    numeric(4,3) NOT NULL CHECK (position_pour BETWEEN -1 AND 1),
    separation       numeric(4,3) NOT NULL CHECK (separation BETWEEN 0 AND 1),
    couverture       numeric(4,3) NOT NULL CHECK (couverture BETWEEN 0 AND 1),
    votants_couverts integer NOT NULL CHECK (votants_couverts >= 0),
    computed_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (scrutin_id, strategy, axis_version)
);

-- 5. Comptes d'administration ----------------------------------------------------------------------
--
-- Aucune adresse IP n'est conservée : le journal doit répondre à « qui a changé quoi », pas tracer
-- des personnes. `display_name` est ce qui s'affiche publiquement à côté d'une catégorisation.

CREATE TABLE admin_user (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    github_id    bigint NOT NULL UNIQUE,
    github_login text   NOT NULL UNIQUE,
    display_name text   NOT NULL,
    actif        boolean NOT NULL DEFAULT true,
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE admin_action (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    admin_user_id bigint REFERENCES admin_user (id),
    action        text NOT NULL,
    target        text,
    detail        jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX admin_action_recent_idx ON admin_action (created_at DESC);

-- 6. Imports ----------------------------------------------------------------------------------------
--
-- Le fichier déposé est conservé tel quel (`contenu`) : c'est la seule façon de rejouer un import
-- contesté, et de garantir que ce qui a été appliqué est bien ce qui a été relu. `apercu` porte le
-- plan calculé au dépôt ; il est recalculé et recomparé au moment d'appliquer.

CREATE TYPE label_import_status AS ENUM ('pending', 'applied', 'rejected');

CREATE TABLE label_import (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    filename       text NOT NULL,
    format         text NOT NULL CHECK (format IN ('csv', 'json')),
    schema_version smallint NOT NULL,
    generateur     text,
    contenu        text NOT NULL,
    content_hash   text NOT NULL,
    status         label_import_status NOT NULL DEFAULT 'pending',
    apercu         jsonb NOT NULL,
    rapport        jsonb,
    uploaded_by    bigint NOT NULL REFERENCES admin_user (id),
    uploaded_at    timestamptz NOT NULL DEFAULT now(),
    decided_by     bigint REFERENCES admin_user (id),
    decided_at     timestamptz
);

-- 7. Les catégorisations ------------------------------------------------------------------------------
--
-- `method` ne vaut que `manual` ou `import` (D3.7) : derrière toute catégorisation, il y a quelqu'un.
-- `reviewed_at` distingue une ligne importée d'une ligne importée **puis relue** : c'est ce couple
-- (`method`, `reviewed_at`) que la règle de conflit de l'import interroge.
--
-- La justification fait au moins dix caractères. Le seuil est arbitraire, la règle ne l'est pas :
-- methodology.md § 5.b dit que le commentaire n'est pas optionnel parce que c'est lui qui sera affiché
-- en explication. « ok » n'explique rien.

CREATE TYPE label_method AS ENUM ('manual', 'import');

CREATE TABLE scrutin_label (
    scrutin_id    bigint   NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    theme_id      smallint NOT NULL REFERENCES theme (id),
    poids         numeric(4,3) NOT NULL CHECK (poids > 0 AND poids <= 1),
    position_pour numeric(4,3) CHECK (position_pour BETWEEN -1 AND 1),
    confiance     numeric(4,3) NOT NULL CHECK (confiance BETWEEN 0 AND 1),
    justification text NOT NULL CHECK (length(btrim(justification)) >= 10),
    method        label_method NOT NULL,
    author_id     bigint NOT NULL REFERENCES admin_user (id),
    import_id     bigint REFERENCES label_import (id),
    reviewed_by   bigint REFERENCES admin_user (id),
    reviewed_at   timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (scrutin_id, theme_id),
    CONSTRAINT scrutin_label_import_renseigne
        CHECK ((method = 'import') = (import_id IS NOT NULL)),
    CONSTRAINT scrutin_label_relecture_complete
        CHECK ((reviewed_by IS NULL) = (reviewed_at IS NULL))
);

CREATE INDEX scrutin_label_theme_idx ON scrutin_label (theme_id, scrutin_id);

-- 8. L'historique --------------------------------------------------------------------------------------
--
-- Une ligne = un acte éditorial sur un scrutin (D3.20), avec l'état complet avant et après. Les
-- tableaux JSONB sont triés par slug de thème à l'écriture, ce qui rend `avant <> apres` structurel :
-- la base refuse d'elle-même d'enregistrer un « changement » qui n'en est pas un. C'est cette
-- contrainte qui fait de « un aller-retour d'export ne crée aucune ligne d'historique » une propriété
-- garantie et non un espoir.

CREATE TABLE label_revision (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    scrutin_id bigint NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    avant      jsonb NOT NULL,
    apres      jsonb NOT NULL,
    method     label_method NOT NULL,
    author_id  bigint NOT NULL REFERENCES admin_user (id),
    import_id  bigint REFERENCES label_import (id),
    motif      text,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT label_revision_change_reel CHECK (avant <> apres)
);

CREATE INDEX label_revision_scrutin_idx ON label_revision (scrutin_id, created_at DESC);

-- 9. Cohérence d'un jeu de catégorisations --------------------------------------------------------------
--
-- Deux règles qu'aucun CHECK de ligne ne peut exprimer : la somme des poids d'un scrutin vaut
-- exactement 1, et une position n'existe que sur un thème qui a un axe. D'où un trigger de
-- contrainte DIFFÉRÉ : les écritures intermédiaires d'un remplacement (DELETE puis INSERT) sont
-- légitimement incohérentes, seul l'état à la validation compte.
--
-- PIÈGE À CONNAÎTRE, il coûte une demi-heure à qui le rencontre sans avertissement : un trigger
-- différé ne se déclenche qu'au COMMIT, et chaque test tourne dans une transaction annulée. Un test
-- qui vérifie ce refus doit forcer la vérification par `SET CONSTRAINTS ALL IMMEDIATE` dans un
-- SAVEPOINT. Écrire ce commentaire dans la fixture de test aussi.

CREATE FUNCTION scrutin_label_coherence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    v_scrutin_id  bigint;
    v_somme       numeric;
    v_incoherents integer;
BEGIN
    IF TG_OP = 'DELETE' THEN
        v_scrutin_id := OLD.scrutin_id;
    ELSE
        v_scrutin_id := NEW.scrutin_id;
    END IF;

    SELECT sum(poids) INTO v_somme FROM scrutin_label WHERE scrutin_id = v_scrutin_id;
    IF v_somme IS NOT NULL AND v_somme <> 1 THEN
        RAISE EXCEPTION 'somme des poids du scrutin % = %, attendu exactement 1', v_scrutin_id, v_somme;
    END IF;

    SELECT count(*) INTO v_incoherents
    FROM scrutin_label sl
    JOIN theme t ON t.id = sl.theme_id
    WHERE sl.scrutin_id = v_scrutin_id
      AND (t.libelle_pole_positif IS NULL) <> (sl.position_pour IS NULL);
    IF v_incoherents > 0 THEN
        RAISE EXCEPTION
            'scrutin % : une position n''existe que sur un thème doté d''un axe, et y est obligatoire',
            v_scrutin_id;
    END IF;

    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER scrutin_label_coherence_trg
    AFTER INSERT OR UPDATE OR DELETE ON scrutin_label
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION scrutin_label_coherence();

-- 10. La file de travail ----------------------------------------------------------------------------------
--
-- Une vue et non une vue matérialisée : ~17 000 lignes filtrées, quelques millisecondes, et une vue
-- matérialisée se périmerait à chaque catégorisation saisie — exactement l'inverse de ce qu'on veut
-- d'une file de travail.
--
-- `rang_priorite` est dérivé du titre parce que c'est le seul signal disponible : `type_code` ne
-- distingue que SPO/SPS/MOC. Mesuré sur le corpus retenu : 216 votes sur l'ensemble d'un texte,
-- 77 articles, 542 amendements, 51 motions. Les titres de l'AN commencent tous par « l'ensemble
-- de… », « l'amendement n° … », « l'article … » — c'est mécanique, pas interprétatif.

CREATE VIEW scrutin_a_categoriser AS
SELECT s.id AS scrutin_id,
       s.an_uid, s.legislature, s.numero, s.date_scrutin, s.titre,
       s.type_code, s.type_libelle, s.sort_code, s.participation,
       LEAST(s.pour, s.contre)::numeric / NULLIF(s.suffrages_exprimes, 0) AS part_minoritaire,
       CASE
           WHEN s.type_code IN ('SPS', 'MOC')  THEN 1
           WHEN s.titre ILIKE 'l''ensemble%'   THEN 2
           WHEN s.titre ILIKE 'l''article%'    THEN 3
           ELSE 4
       END AS rang_priorite,
       EXISTS (SELECT 1 FROM scrutin_label sl WHERE sl.scrutin_id = s.id) AS est_categorise
FROM scrutin s
CROSS JOIN corpus_parametre c
WHERE s.chambre = 'assemblee'
  AND s.participation >= c.participation_min;
