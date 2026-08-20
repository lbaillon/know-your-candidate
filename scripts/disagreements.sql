-- Contrôle croisé entre les catégorisations enregistrées et la mesure automatique d'axe.
--
-- À jouer APRÈS un import (voir scripts/README.md) :
--     podman compose exec -T postgres psql -U kyc -d kyc -f - < scripts/disagreements.sql
--
-- Pourquoi ce contrôle et pas un échantillon au hasard : l'estimation d'axe est une mesure faite
-- sur les votes réels des groupes (D3.7), indépendante du titre du texte — donc indépendante de
-- ce sur quoi un modèle ou un relecteur s'appuie. Quand les deux se contredisent, l'un des deux se
-- trompe, et c'est exactement là qu'une relecture humaine rapporte le plus.
--
-- Ce que ce désaccord ne prouve PAS : que la catégorisation est fausse. Un texte de « niche »
-- soutenu par une coalition inhabituelle produit une mesure surprenante sans que la lecture
-- thématique soit en cause. C'est une file de relecture, pas un verdict.

\echo '=== 1. Taux d’accord (mesure 6 du plan de la phase 3)'
\echo 'Compare le SIGNE de la position saisie à celui de la position mesurée, sur les scrutins qui'
\echo 'ont les deux. Les positions nulles des deux côtés sont exclues : un signe nul ne s’oppose à rien.'

-- Décomposé, et jamais rendu en un seul chiffre : la moyenne mélange des populations où la
-- comparaison a un sens et d'autres où elle n'en a aucun. Mesuré le 19/08/2026, l'agrégat valait
-- 48,5 % — indiscernable du hasard — alors que le sous-ensemble comparable était à 68,4 %. Un
-- taux global est ici un chiffre faux, pas un résumé.
WITH comparables AS (
    SELECT t.slug,
           l.position_pour AS saisie,
           e.position_pour AS mesuree,
           t.libelle_pole_negatif IS NOT NULL
               AND t.slug NOT IN ('institutions-democratie', 'agriculture', 'europe')
               AS axe_gauche_droite
    FROM scrutin_label l
    JOIN theme t ON t.id = l.theme_id
    JOIN scrutin_axis_estimate e ON e.scrutin_id = l.scrutin_id
    JOIN group_axis g ON g.version = e.axis_version AND g.is_current
    WHERE l.position_pour IS NOT NULL
      AND l.position_pour <> 0
      AND e.position_pour <> 0
)
SELECT CASE WHEN axe_gauche_droite THEN 'axes gauche-droite (comparable)'
            ELSE 'axes non gauche-droite (hors sujet)' END AS sous_ensemble,
       count(*)                                            AS comparables,
       count(*) FILTER (WHERE sign(saisie) = sign(mesuree)) AS accords,
       round(100.0 * count(*) FILTER (WHERE sign(saisie) = sign(mesuree)) / count(*), 1) AS pct
FROM comparables
GROUP BY 1
ORDER BY 2 DESC;

\echo ''
\echo '--- le même détail, thème par thème'

SELECT t.slug,
       count(*)                                                             AS comparables,
       count(*) FILTER (WHERE sign(l.position_pour) = sign(e.position_pour)) AS accords,
       round(100.0 * count(*) FILTER (WHERE sign(l.position_pour) = sign(e.position_pour))
             / count(*), 1)                                                 AS pct
FROM scrutin_label l
JOIN theme t ON t.id = l.theme_id
JOIN scrutin_axis_estimate e ON e.scrutin_id = l.scrutin_id
JOIN group_axis g ON g.version = e.axis_version AND g.is_current
WHERE l.position_pour IS NOT NULL
  AND l.position_pour <> 0
  AND e.position_pour <> 0
GROUP BY 1
ORDER BY 2 DESC;

\echo ''
\echo '=== 2. File de relecture : les désaccords, les plus fiables en premier'
\echo 'Triés par séparation décroissante — une mesure qui sépare nettement les deux camps est celle'
\echo 'dont le désaccord mérite le plus qu’on aille voir.'

SELECT s.legislature,
       s.numero,
       t.slug                        AS theme,
       l.position_pour               AS position_saisie,
       e.position_pour               AS position_mesuree,
       e.separation,
       l.method,
       (l.reviewed_at IS NOT NULL)   AS relue,
       left(s.titre, 90)             AS titre
FROM scrutin_label l
JOIN scrutin s ON s.id = l.scrutin_id
JOIN theme t ON t.id = l.theme_id
JOIN scrutin_axis_estimate e ON e.scrutin_id = l.scrutin_id
JOIN group_axis g ON g.version = e.axis_version AND g.is_current
WHERE l.position_pour IS NOT NULL
  AND l.position_pour <> 0
  AND e.position_pour <> 0
  AND sign(l.position_pour) <> sign(e.position_pour)
ORDER BY e.separation DESC, abs(e.position_pour) DESC
LIMIT 50;

\echo ''
\echo '=== 3. Couverture : où en est la catégorisation du corpus de travail'

SELECT count(*)                                     AS corpus,
       count(*) FILTER (WHERE est_categorise)       AS categorises,
       count(*) FILTER (WHERE NOT est_categorise)   AS restants
FROM scrutin_a_categoriser;
