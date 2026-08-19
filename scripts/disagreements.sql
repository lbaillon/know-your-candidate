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

SELECT count(*)                                                         AS comparables,
       count(*) FILTER (WHERE sign(l.position_pour) = sign(e.position_pour)) AS accords,
       round(
           100.0 * count(*) FILTER (WHERE sign(l.position_pour) = sign(e.position_pour))
           / nullif(count(*), 0),
           1
       )                                                                AS taux_accord_pct,
       count(*) FILTER (WHERE l.method = 'manual')                      AS dont_saisies_a_la_main,
       count(*) FILTER (WHERE l.reviewed_at IS NOT NULL)                AS dont_relues
FROM scrutin_label l
JOIN scrutin_axis_estimate e ON e.scrutin_id = l.scrutin_id
JOIN group_axis g ON g.version = e.axis_version AND g.is_current
WHERE l.position_pour IS NOT NULL
  AND l.position_pour <> 0
  AND e.position_pour <> 0;

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
