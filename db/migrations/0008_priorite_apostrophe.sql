-- Phase 3.0 — F8 : l'apostrophe typographique reléguait des votes sur l'ensemble d'un texte au
-- dernier rang de la file de travail.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md).
--
-- Le `rang_priorite` de `scrutin_a_categoriser` (migration 0007) compare le début du titre à
-- `'l''ensemble%'`, avec une apostrophe droite (U+0027). Or l'open data de l'Assemblée mélange les
-- deux apostrophes : 118 titres sur les 16 956 de l'Assemblée commencent par « l’ » typographique
-- (U+2019).
--
-- Impact réel, mesuré le 19/08/2026 et cité tel quel plutôt qu'arrondi vers le haut : sur le corpus
-- de travail actuel (seuil de participation à 50 %), **deux scrutins** étaient mal classés — 16/3048
-- (vote sur l'ensemble d'une proposition de résolution) et 16/3047 (son article unique), tous deux
-- relégués au rang 4, celui des amendements, donc en toute fin de file. Les neuf autres titres
-- concernés du corpus sont des scrutins solennels, déjà au rang 1 par leur `type_code`, que le
-- défaut n'atteignait donc pas.
--
-- Deux scrutins, cela ne justifierait pas une migration à soi seul ; ce qui la justifie, c'est que
-- le défaut grandit avec le corpus. Sur l'ensemble des scrutins de l'Assemblée, 12 votes sur un
-- texte entier et 14 articles sont dans ce cas : abaisser `corpus_parametre.participation_min`,
-- ce qui est prévu, les ferait entrer d'un coup dans la file, mal classés.
--
-- Rien n'était faux à l'écran, et c'est bien le problème : une file de travail mal ordonnée ne
-- ressemble pas à une panne, elle ressemble à une file de travail.
--
-- Une classe de caractères plutôt qu'un `replace()` : elle dit ce qu'on accepte, là où une
-- normalisation dirait ce qu'on remplace, et la source peut très bien introduire demain une
-- troisième variante qu'il faudra voir plutôt que d'absorber.

CREATE OR REPLACE VIEW scrutin_a_categoriser AS
SELECT s.id AS scrutin_id,
       s.an_uid, s.legislature, s.numero, s.date_scrutin, s.titre,
       s.type_code, s.type_libelle, s.sort_code, s.participation,
       LEAST(s.pour, s.contre)::numeric / NULLIF(s.suffrages_exprimes, 0) AS part_minoritaire,
       CASE
           WHEN s.type_code IN ('SPS', 'MOC')     THEN 1
           WHEN s.titre ~* '^l[''’]ensemble'      THEN 2
           WHEN s.titre ~* '^l[''’]article'       THEN 3
           ELSE 4
       END AS rang_priorite,
       EXISTS (SELECT 1 FROM scrutin_label sl WHERE sl.scrutin_id = s.id) AS est_categorise
FROM scrutin s
CROSS JOIN corpus_parametre c
WHERE s.chambre = 'assemblee'
  AND s.participation >= c.participation_min;
