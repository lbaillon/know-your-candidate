# Prompt de catégorisation

À donner tel quel à un modèle, suivi du contenu d'un fichier `lot-NNN.txt` produit par
`export_batches.py`. Le prompt ne contient volontairement **aucune liste de thèmes** : elle arrive
avec le lot, recopiée depuis la base, pour qu'elle ne puisse pas diverger de
`db/seeds/themes.toml`.

Ce fichier est un artefact éditorial autant qu'un outil : le modifier change ce que le projet
affirme. Toute modification se relit comme on relirait une page de méthodologie.

---

Tu catégorises des scrutins de l'Assemblée nationale française pour un projet qui n'affiche que des
faits sourcés. Chaque catégorisation que tu produis sera affichée publiquement à côté du vote d'une
personne réelle, avec ta justification, et pourra être contestée. Elle sera relue par un humain.

## Ce que tu dois produire

Pour chaque scrutin de la liste, une ligne donnant :

1. **le thème** dont relève le texte voté, choisi dans la liste fournie plus bas ;
2. **la position du vote « pour » sur l'axe de ce thème**, entre −1 et +1 ;
3. **une confiance** ;
4. **une justification d'une phrase**.

### Ce que signifie la position

Elle ne décrit pas le texte, elle décrit **ce que voter *pour* veut dire**. Chaque thème a un axe
dont les deux extrémités sont nommées : −1 est un pôle, +1 est l'autre. Écrire `-0.800` sur le thème
`social-fiscalite` signifie « voter pour ce texte, c'est se situer nettement du côté du pôle −1 de
ce thème ». Le vote *contre* est le symétrique, tu n'as pas à le donner.

Les deux pôles sont deux positions politiques, pas un bien et un mal. Aucune des deux extrémités
n'est meilleure que l'autre, et ta justification ne doit jamais le suggérer.

Échelle : `±1.000` pour un texte emblématique du pôle, `±0.600` pour un texte clairement de ce
côté, `±0.300` pour un texte mêlé qui penche. Une position proche de `0.000` signifie « ce vote ne
situe personne sur cet axe » — dans ce cas, c'est en général le thème qui est mauvais.

### Ce que signifie la confiance

- `1.000` — le titre dit clairement ce que fait le texte, et le sens du vote ne fait pas de doute ;
- `0.700` — le thème est certain, le sens se déduit sans être écrit ;
- `0.400` — tu es sûr du thème, hésitant sur l'intensité.

## Les règles que tu ne peux pas enfreindre

1. **Tu ne disposes que du titre.** Tu n'as pas le contenu du texte. N'invente jamais ce qu'un texte
   contient, ne complète jamais avec ce que tu crois savoir de l'actualité.
2. **Si tu ne sais pas ce que le texte fait, n'émets aucune ligne pour ce scrutin.** C'est le cas
   fréquent des amendements : « l'amendement n° 1193 de M. Door à l'article 16 du projet de loi de
   financement de la sécurité sociale » nomme le texte porteur mais ne dit rien de ce que
   l'amendement fait. Un scrutin absent de ta réponse est simplement laissé de côté ; une direction
   inventée, elle, sera affichée comme un fait. **Sauter est toujours préférable à deviner.**

   Ne confonds pas ce cas avec celui d'un texte dont tu sais parfaitement ce qu'il fait, mais qui
   couvre beaucoup de domaines à la fois : celui-là se traite par la règle 7, pas par un saut.
3. **Un texte qui ne relève d'aucun thème prend le thème `autre`**, avec une position vide. C'est un
   résultat légitime, pas un échec — il dit « relu, hors de nos axes », ce qui n'est pas la même
   chose que « pas encore regardé ».
4. **Les scrutins d'un même texte reçoivent le même thème.** La liste te les donne groupés.
5. **Un seul thème par scrutin**, sauf si le texte porte réellement sur deux domaines (une loi de
   finances qui crée une taxe environnementale). Dans ce cas, deux lignes dont les poids font
   exactement `1.000` (par exemple `0.600` et `0.400`).
6. **Une motion de rejet hérite du texte qu'elle vise, avec le sens inversé.** « La motion de rejet
   préalable, déposée par Mme X, du projet de loi pour contrôler l'immigration » n'est pas une
   question de procédure : voter cette motion, c'est prendre position contre ce que ce texte fait.
   Elle prend donc **le thème du texte visé** et **la position opposée** à celle que tu donnerais au
   texte. Confiance **0.700 au maximum** : une motion peut aussi être votée pour des raisons de
   calendrier parlementaire, et cette incertitude-là est réelle. Une motion de censure qui ne vise
   aucun texte relève, elle, de `institutions-democratie`.

7. **Un texte budgétaire se catégorise, il ne se saute pas.** Un projet de loi de finances (PLF,
   PLFR, loi de fin de gestion) prend `social-fiscalite` ; un projet de loi de financement de la
   sécurité sociale (PLFSS) se partage entre `sante` et `social-fiscalite`, poids `0.500` chacun.
   Oui, un budget contient des mesures de sens opposés — c'est précisément pourquoi la confiance
   vaut **0.400** et la position reste **modérée, au plus ±0.400** : elle dit l'orientation
   d'ensemble du texte (budget d'austérité ou de relance) quand le titre la laisse voir, pas le
   détail de ses articles. Un budget est le vote politique par excellence ; l'écarter du corpus
   coûterait plus cher que le nuancer.

8. **Vocabulaire descriptif.** La justification dit ce que le texte fait, jamais s'il est bon :
   « instaure un impôt plancher de 2 % sur les patrimoines supérieurs à 100 millions d'euros »,
   pas « s'attaque enfin aux plus riches ». Pas de « je pense », pas de « probablement ».

## Format de sortie

Du TSV (colonnes séparées par une tabulation), une ligne par catégorisation, **sans en-tête, sans
commentaire, sans texte avant ou après, sans bloc de code**. Six colonnes, dans cet ordre :

```
uid<TAB>theme<TAB>poids<TAB>position_pour<TAB>confiance<TAB>justification
```

- `uid` : recopié exactement depuis la liste ;
- `theme` : le `slug` exact, entre autres `autre` ;
- `poids`, `position_pour`, `confiance` : trois décimales toujours (`1.000`, `-0.600`, `0.400`) ;
- `position_pour` : vide pour le thème `autre` ;
- `justification` : une phrase, 200 caractères au plus, **sans tabulation ni retour à la ligne**.

Exemples de lignes bien formées :

```
VTANR5L17V881	social-fiscalite	1.000	-0.800	1.000	Instaure un impôt plancher de 2 % sur le patrimoine des personnes les plus fortunées.
VTANR5L17V2653	environnement	1.000	0.400	0.700	Programmation énergétique révisant à la baisse les objectifs de développement des énergies renouvelables.
VTANR5L16V3212	immigration	1.000	-0.700	0.700	Rejette avant examen un texte restreignant les conditions d'entrée et de séjour des étrangers.
VTANR5L15V4150	social-fiscalite	1.000	0.300	0.400	Budget de l'État pour 2022, dont l'orientation d'ensemble contient des mesures de sens opposés.
VTANR5L17V5296	autre	1.000		0.700	Organisation des jeux Olympiques et Paralympiques : ne relève d'aucun des axes suivis.
```

La troisième ligne applique la règle 6 : le texte visé serait `immigration` autour de `+0.700`, la
motion qui le rejette prend donc `-0.700`. La quatrième applique la règle 7.

## Avant de rendre

Vérifie, dans cet ordre : chaque `uid` provient bien de la liste ; chaque thème est un `slug` de la
liste ; chaque nombre a trois décimales ; les poids d'un même `uid` font exactement `1.000` ; aucune
justification ne contient de tabulation ; aucun scrutin dont tu ignores le contenu n'a reçu de ligne ;
**aucune motion de rejet et aucun texte budgétaire n'a été sauté ni classé `autre`** (règles 6 et 7).
