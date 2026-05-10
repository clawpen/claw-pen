# Claw Pen — Garde-fous et modèle de menaces
## Comment la plateforme reste sécuritaire sans dicter la pédagogie

---

### Principe directeur

L'IA est un outil. Comme la calculatrice, le dictionnaire, ou Wikipédia, l'**utilisation appropriée se définit par l'enseignant, pas par la plateforme**. Demander à l'agent de rédiger un texte est un usage valide; ce que l'élève fait avec ce texte, et ce qu'on en apprend, est une décision pédagogique de l'enseignant.

La plateforme n'impose pas une morale d'usage. Elle fait deux choses : (1) elle exécute fidèlement les balises que l'enseignant a tracées, et (2) elle maintient un plancher de sécurité non négociable pour la protection des mineurs.

Métaphore : les rails de quille pour enfants, ou l'antenne de Mario Kart 8. Les garde-fous gardent l'élève sur la piste assez longtemps pour qu'il y ait apprentissage. **L'enseignant trace où va la piste; la plateforme protège les rails.**

---

### Trois couches de protection

#### 1. Garde-fous pédagogiques *(rédigés par l'enseignant)*

L'enseignant écrit le comportement de son agent dans `system_prompt.md`, `boundaries.md` et `examples.md` (volume identité). Ces fichiers définissent : ton, expertise, ce que l'agent fait, ce qu'il refuse, comment il guide. Modifiables en tout temps par l'enseignant; **immuables côté élève**.

#### 2. Plancher de sécurité *(non négociable, applicable à tous les agents)*

Indépendamment des choix de l'enseignant, la plateforme refuse et signale :
- Idéation suicidaire ou automutilation → message de soutien standard, escalade immédiate à l'enseignant et au service Jeunesse, J'écoute (1‑800‑668‑6868).
- Harcèlement ou menaces visant un pair, un membre du personnel, un parent.
- Contenu sexuel impliquant un mineur (toujours).
- Production de contenu illégal (drogues, armes, fraude).

Ces refus sont assurés par un classificateur local (LlamaGuard ou équivalent) tournant sur la même infrastructure d'inférence que le modèle principal — aucun envoi à des serveurs étatsuniens.

#### 3. Sécurité technique *(invisible, infrastructurelle)*

Conteneurs Docker isolés · volume identité monté en lecture seule · authentification JWT par utilisateur · sessions signées (un élève ne peut pas lire les conversations d'un autre) · limitation de débit (30 messages/min, 200/h par élève) · journaux audit horodatés · clés API jamais visibles aux élèves · pas d'accès shell agent → hôte.

---

### Tableau des menaces

| Menace | Atténuation | Couche |
|---|---|---|
| Injection de consigne (« ignore les instructions précédentes ») | Système de re-chargement du `system_prompt` à chaque tour; classificateur de sortie qui détecte les fuites de rôle | 1 |
| Extraction du prompt enseignant | Clause explicite dans le prompt; on suppose que le prompt sera vu par les élèves — pas de secret dedans | 1 |
| Élève demande quelque chose hors-portée du cours | L'enseignant écrit dans `boundaries.md` ce qui est permis ou non — la plateforme respecte sa décision | 1 |
| Idéation suicidaire, automutilation | Classificateur local + escalade enseignant + ressources crise | 2 |
| Harcèlement, contenu visant un pair/employé | Classificateur + journal audit + alerte enseignant | 2 |
| Contenu sexuel impliquant un mineur | Refus catégorique, journalisation, aucune exception | 2 |
| Lecture des conversations d'autres élèves | Sessions liées à l'identité authentifiée, signature non falsifiable | 3 |
| Surcharge serveur (DoS) | Limitation de débit par utilisateur, plafond de concurrence par classe | 3 |
| Exécution de code sur le serveur via l'IA | Conteneurisation, capacités Linux restreintes, lecture seule du système de fichiers racine | 3 |
| Vol de clés API | Clés stockées chiffrées sur le serveur, jamais transmises au navigateur | 3 |
| Extraction d'information personnelle d'autres élèves | Aucun PII dans les prompts; agent n'a pas accès aux dossiers scolaires | 3 |
| Usage durant un examen sans autorisation | L'enseignant désactive ses agents par horaire, ou installe un volume identité « mode-examen » | 1 |

---

### Ce que ce document ne couvre pas *(honnêteté requise pour le pilote)*

- **Détection de plagiat dans les rédactions** — hors portée. Si la politique du conseil scolaire l'exige, intégration avec un service externe (Compilatio, etc.) est en feuille de route v2.
- **Conformité AODA / WCAG AA complète** — interface en cours, niveau A visé pour septembre, AA pour janvier 2027.
- **Conformité FIPPA pour archivage long terme** — politique de rétention configurable, mais l'avis légal du conseil scolaire prime sur les défauts de la plateforme.
- **Vérification d'identité forte** — l'authentification du pilote utilise des comptes locaux; intégration avec les comptes du conseil (Brightspace, Active Directory) est une étape v1.5 → v2.

---

### Responsabilité partagée

| Couche | Qui est responsable |
|---|---|
| 1 — Garde-fous pédagogiques | Enseignant |
| 2 — Plancher de sécurité | Claw Pen + Collège Boréal (hébergeur du classificateur) |
| 3 — Sécurité technique | Claw Pen (logiciel) + IT scolaire (réseau, déploiement) |

Le pilote inclut une formation enseignant d'une heure sur la rédaction du `system_prompt.md` et du `boundaries.md`. La plateforme fournit des modèles francophones de départ pour matières communes (français, mathématiques, sciences humaines).
