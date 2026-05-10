# Claw Pen
## Une infrastructure d'IA souveraine pour l'éducation francophone du Nord de l'Ontario

*Projet pilote : partenariat entre le Collège Boréal et une école secondaire francophone*

---

### Le contexte

Les écoles secondaires francophones du Nord de l'Ontario font face à un choix difficile en matière d'intelligence artificielle. Adopter les outils étatsuniens disponibles (ChatGPT Edu, Khanmigo, Copilot) signifie transmettre les conversations et les données de mineurs vers des serveurs hors du pays, dans une langue où ces produits demeurent moins performants qu'en anglais. Le coût récurrent par élève — environ 75 000 $ par année pour une école de 250 élèves — est par ailleurs prohibitif pour la majorité des conseils scolaires.

### La proposition

Claw Pen est une plateforme à code source ouvert, conçue localement, qui permet à un établissement d'héberger ses propres agents d'intelligence artificielle spécialisés — tuteur de mathématiques, correcteur de rédaction française, assistant de recherche — configurés par les enseignants eux-mêmes. Aucune conversation ne quitte l'infrastructure publique ontarienne francophone.

### L'architecture proposée

- Le **Collège Boréal** héberge l'infrastructure d'inférence (grappe GPU) dans son centre de données existant.
- Chaque **école secondaire cliente** déploie un serveur d'orchestration de petite taille (~5 000 $) qui se connecte à l'infrastructure du Collège.
- Les **enseignants** configurent leurs propres agents par interface graphique : ton, expertise, balises pédagogiques.
- Les **conversations** sont enregistrées localement à l'école; aucune donnée d'élève ne quitte le réseau francophone public.

### Les avantages distinctifs

- **Souveraineté des données** : les conversations d'élèves mineurs ne sortent jamais de l'infrastructure publique francophone ontarienne.
- **Français de premier rang** : modèles ouverts performants en français (Qwen, Mistral, éventuellement Kimi), sans la dégradation typique des produits anglophones.
- **Contrôle pédagogique enseignant** : chaque enseignant définit le comportement, le ton et les limites de ses propres agents.
- **Observabilité de la classe** : l'enseignant peut suivre en temps réel les difficultés de ses élèves et intervenir au besoin.
- **Modèle économique** : investissement en capital ponctuel (~5 000 $ par école) plutôt qu'abonnement perpétuel par élève.

### Pourquoi le Collège Boréal

Cette architecture s'aligne directement avec le mandat existant du Collège — recherche appliquée, service à la communauté francophone régionale, formation en technologies de l'information. Le projet offre :

- une plateforme de recherche appliquée concrète pour les programmes en TI;
- un service régional aux conseils scolaires francophones (CSCNO, CSPGNO);
- l'admissibilité à des sources de financement existantes : **FedNor**, **Plan d'action pour les langues officielles**, **Programme d'innovation dans les collèges et la communauté du CRSNG**.

### Projet pilote proposé (septembre – décembre 2026)

- Une école francophone partenaire, un enseignant volontaire, une classe ciblée.
- Un cas d'usage précis : tuteur de français ou de mathématiques.
- Documentation rigoureuse des résultats : heures-enseignant économisées, engagement des élèves, résultats d'apprentissage qualitatifs et quantitatifs.
- Évaluation en fin de semestre, en vue d'un élargissement à l'hiver 2027.

### Prochaines étapes

1. Démonstration technique fonctionnelle, disponible immédiatement.
2. Rencontre exploratoire avec le Bureau de la recherche appliquée du Collège Boréal.
3. Identification de l'école pilote et de l'enseignant partenaire.
4. Planification budgétaire selon les sources de financement applicables.

---

**Contact**

[Votre nom]
[Courriel] · [Téléphone]
[Lien démo / dépôt logiciel — facultatif]
