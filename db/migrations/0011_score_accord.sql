-- Phase 4.1 — F1 et F2 : une contribution dont les deux lectures se contredisent ne compte pas, et
-- un thème dont l'axe ne se lit pas gauche-droite ne produit pas d'orientation publique.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md).
--
-- Numérotée 0011 et non 0010 comme l'esquissait le plan (phase-4.1-partis-scores.md) : 0010 est
-- déjà pris par `0010_scores_vues.sql` (vues matérialisées de la phase 4), mergée avant que ce
-- correctif ne soit écrit. Les migrations sont immuables une fois mergées, donc pas de renumérotage.

-- 1. Quels axes la mesure peut-elle arbitrer ? (F2) -----------------------------------------------
--
-- L'estimation d'axe situe un scrutin sur l'axe gauche-droite des groupes. Elle peut donc contredire
-- utilement une catégorisation sur `securite` ou `social-fiscalite` ; elle ne dit rien sur l'Europe
-- (intégration contre souveraineté), l'agriculture ou les institutions, dont les axes sont
-- orthogonaux à celui-là. La réserve était écrite dans db/seeds/themes.toml depuis la phase 3 ;
-- elle devient une colonne, parce qu'elle commande maintenant du calcul et de l'affichage.
--
-- `DEFAULT true` puis correction explicite : un thème ajouté plus tard sans y penser serait traité
-- comme gauche-droite, donc filtré par F1 — le comportement prudent. Le seed, lui, rend le champ
-- obligatoire : ajouter un thème force à répondre à la question.

ALTER TABLE theme ADD COLUMN axe_gauche_droite boolean NOT NULL DEFAULT true;

UPDATE theme SET axe_gauche_droite = false
WHERE slug IN ('institutions-democratie', 'agriculture', 'europe', 'autre');

-- 2. Pourquoi une contribution ne pèse pas (F1) ----------------------------------------------------
--
-- Une contribution écartée reste écrite : elle est la trace de ce qui a été regardé puis mis de côté,
-- et l'explication l'affiche avec sa raison. C'est la différence entre « ce vote ne compte pas » et
-- « ce vote n'existe pas », que le projet ne confond jamais.

CREATE TYPE contribution_exclusion AS ENUM ('abstention', 'desaccord_mesure');

ALTER TABLE score_contribution ADD COLUMN exclusion contribution_exclusion;

-- Rétro-remplissage AVANT la contrainte ci-dessous, absent du plan mais nécessaire : la base réelle
-- porte déjà 18 108 contributions d'abstention (runs de la phase 4), toutes avec `exclusion` NULL
-- puisque la colonne vient d'apparaître. Une contrainte CHECK ajoutée par ALTER TABLE valide par
-- défaut les lignes existantes ; sans ce rétro-remplissage, la contrainte de la section suivante
-- casserait `make migrate` sur toute base non vide dès sa création.
UPDATE score_contribution SET exclusion = 'abstention' WHERE position = 'abstention';

-- Toute exclusion annule le poids. L'inverse n'est pas vrai : un poids nul peut aussi venir d'une
-- bipolarité de 1 (D4.9), qui n'est pas une exclusion mais une pondération qui tombe à zéro.
ALTER TABLE score_contribution
    ADD CONSTRAINT score_contribution_exclusion_sans_poids
        CHECK (exclusion IS NULL OR poids = 0);

-- PIÈGE À NE PAS REPRODUIRE : écrire cette contrainte
--     CHECK ((position = 'abstention') = (exclusion = 'abstention'))
-- laisserait passer une abstention sans exclusion, parce que `exclusion = 'abstention'` vaut NULL
-- quand la colonne est NULL, que la comparaison entière vaut alors NULL, et qu'un CHECK à NULL
-- passe. `IS NOT DISTINCT FROM` compare en traitant NULL comme une valeur.
ALTER TABLE score_contribution
    ADD CONSTRAINT score_contribution_abstention_toujours_exclue
        CHECK ((position = 'abstention') = (exclusion IS NOT DISTINCT FROM 'abstention'));

-- 3. Ce que l'orientation doit pouvoir dire ---------------------------------------------------------

ALTER TABLE person_theme_score
    ADD COLUMN ecartes_desaccord integer NOT NULL DEFAULT 0 CHECK (ecartes_desaccord >= 0);
