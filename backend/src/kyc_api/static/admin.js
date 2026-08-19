/*
 * Back-office — coloration du pointeau du curseur de position, et rien d'autre.
 *
 * C'est une amélioration progressive, au sens strict : sans ce fichier, le formulaire de
 * catégorisation reste entièrement utilisable (D3.17), le pointeau restant simplement neutre au
 * lieu de prendre la couleur du dégradé à sa position. Aucun calcul métier, aucune requête, aucune
 * donnée lue ailleurs que dans la valeur du champ lui-même.
 *
 * Les trois couleurs répètent celles du dégradé de la piste (45-admin.css) : si l'une bouge,
 * l'autre doit bouger avec. Elles ne sont pas dans un jeton partagé parce qu'un `var()` de feuille
 * de style ne se lit pas commodément ici, et parce que les couleurs politiques ne doivent pas
 * quitter le back-office (D2.11).
 */
(function () {
  "use strict";

  var GAUCHE = [204, 0, 0]; /* #cc0000 */
  var CENTRE = [232, 232, 228]; /* #e8e8e4 */
  var DROITE = [13, 59, 138]; /* #0d3b8a */

  var SELECTEUR = 'input[type="range"][name^="position_"]';

  function melange(depuis, vers, t) {
    return depuis.map(function (composante, i) {
      return Math.round(composante + (vers[i] - composante) * t);
    });
  }

  function couleur(valeur) {
    if (isNaN(valeur)) {
      return null;
    }
    var t = Math.max(-1, Math.min(1, valeur));
    var rgb = t < 0 ? melange(CENTRE, GAUCHE, -t) : melange(CENTRE, DROITE, t);
    return "rgb(" + rgb.join(", ") + ")";
  }

  function applique(input) {
    var valeur = couleur(parseFloat(input.value));
    if (valeur === null) {
      input.style.removeProperty("--position-color");
    } else {
      input.style.setProperty("--position-color", valeur);
    }
  }

  function initialise(racine) {
    if (!racine || !racine.querySelectorAll) {
      return;
    }
    Array.prototype.forEach.call(racine.querySelectorAll(SELECTEUR), applique);
  }

  document.addEventListener("input", function (evenement) {
    var cible = evenement.target;
    if (cible instanceof Element && cible.matches(SELECTEUR)) {
      applique(cible);
    }
  });

  // Les fragments HTMX (ajout d'un second thème, passage au scrutin suivant) remplacent le
  // formulaire : les champs échangés n'ont jamais reçu la couleur de leur valeur initiale.
  document.addEventListener("htmx:afterSwap", function (evenement) {
    initialise(evenement.target);
  });

  initialise(document);
})();
