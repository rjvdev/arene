#!/usr/bin/env bash
# Test de bout en bout de l'interface : serveur temporaire (port 18080), runs simulés (aucun GPU).
# Usage : bash tests/essai.sh   (après cargo build --release)
set -u
ICI="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ICI/target/release/arene"
T="$(mktemp -d)"
PORT=18080
U="http://127.0.0.1:$PORT"
J="$T/cookies"
OK=0; KO=0
verifie() { if [ "$2" = "$3" ]; then echo "  ok   $1"; OK=$((OK+1)); else echo "  ÉCHEC $1 (attendu « $3 », obtenu « $2 »)"; KO=$((KO+1)); fi; }
contient() { if printf '%s' "$2" | grep -q -- "$3"; then echo "  ok   $1"; OK=$((OK+1)); else echo "  ÉCHEC $1 (« $3 » absent)"; KO=$((KO+1)); fi; }
code() { curl -s -o /dev/null -w '%{http_code}' "$@"; }
dernier_run() { ls "$T/donnees/evolution/"*.json 2>/dev/null | sort | tail -1; }
attendre_fin() { for _ in $(seq 50); do f=$(dernier_run); [ -n "$f" ] && python3 -c "import json,sys;sys.exit(0 if not json.load(open('$f'))['en_cours'] else 1)" 2>/dev/null && return; sleep 0.2; done; }

cp -r "$ICI/contenu" "$ICI/agents-locaux" "$T/"
python3 - "$T/agents-locaux/evolution.toml" "$PORT" <<'PY'
import sys, re
p, port = sys.argv[1], sys.argv[2]
s = open(p).read()
s = re.sub(r'(?m)^adresse_serveur = .*\n', '', s)
s = re.sub(r'(?m)^connexion_locale_automatique = .*\n', '', s)
s = f'adresse_serveur = "127.0.0.1:{port}"\nconnexion_locale_automatique = false\n' + s
open(p, 'w').write(s)
PY
"$BIN" "$T" >"$T/serveur.log" 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null; rm -rf "$T"' EXIT
for _ in $(seq 50); do curl -s -o /dev/null "$U/" && break; sleep 0.1; done
CLE="$(cat "$T/donnees/cle-admin.txt")"

echo "Pages"
for p in / /runs /parametres /connexion /graphiques.js; do verifie "GET $p" "$(code "$U$p")" "200"; done
contient "accueil vide explicite" "$(curl -s "$U/")" "Aucune évolution"

echo "Anciennes fonctions supprimées"
for p in /api/join /api/submit /llms.txt /openapi.json /communs /api/communs/rejoindre /participants /memorial /api/state; do
  verifie "$p n'existe plus" "$(code "$U$p")" "404"
done
verifie "/evolution redirige vers l'accueil" "$(code "$U/evolution")" "308"

echo "Droits"
verifie "lancement refusé sans connexion" "$(curl -s -o /dev/null -w '%{redirect_url}' -X POST "$U/evolution/lancer" -d mode=sim | grep -c connexion)" "1"
verifie "fichiers refusés sans connexion" "$(curl -s -o /dev/null -w '%{redirect_url}' "$U/fichiers" | grep -c connexion)" "1"
verifie "un faux en-tête de tunnel ne donne pas l'accès local" "$(curl -s -H 'X-Forwarded-For: 1.2.3.4' "$U/" | grep -c 'Lancer une évolution')" "0"
verifie "mauvaise clé refusée" "$(curl -s -o /dev/null -w '%{redirect_url}' -X POST "$U/connexion" -d cle=faux | grep -c incorrecte)" "1"
curl -s -c "$J" -o /dev/null -X POST "$U/connexion" --data-urlencode "cle=$CLE"
contient "connecté : formulaire de lancement visible" "$(curl -s -b "$J" "$U/")" "Lancer une évolution"

echo "Lancer une évolution (simulée) avec question imposée"
curl -s -b "$J" -o /dev/null -X POST "$U/evolution/lancer" --data-urlencode mode=sim --data-urlencode generations=5 \
  --data-urlencode population=6 --data-urlencode "question=Combien font 37 × 41 ?" --data-urlencode reponse=1517
attendre_fin; R1=$(dernier_run); ID1=$(basename "$R1" .json)
verifie "journal écrit, 5 générations" "$(python3 -c "import json;print(len(json.load(open('$R1'))['generations']))")" "5"
verifie "question imposée utilisée" "$(python3 -c "import json;print(json.load(open('$R1'))['generations'][0]['attendu'])")" "1517"
verifie "paramètres enregistrés dans le run" "$(python3 -c "import json;print('taux_piege' in json.load(open('$R1'))['config'])")" "True"
verifie "générations prévues enregistrées" "$(python3 -c "import json;print(json.load(open('$R1'))['generations_prevues'])")" "5"
verifie "tableau de bord du run" "$(code -b "$J" "$U/?run=$ID1")" "200"
contient "API de suivi" "$(curl -s "$U/api/evolution?run=$ID1")" "\"run_id\":\"$ID1\""

echo "Paramètres"
AVANT=$(cat "$T/agents-locaux/evolution.toml")
curl -s -b "$J" -o "$T/rep.html" -X POST "$U/parametres" -d 'formulaire=1&taille=4&survivants=9'
contient "incohérence refusée avec explication" "$(cat "$T/rep.html")" "survivants doit être"
verifie "fichier inchangé après refus" "$(cat "$T/agents-locaux/evolution.toml")" "$AVANT"
verifie "valeur valide acceptée" "$(curl -s -b "$J" -o /dev/null -w '%{redirect_url}' -X POST "$U/parametres" -d 'taux_piege=0.5' | grep -c 'Enregistr')" "1"
contient "fichier mis à jour" "$(cat "$T/agents-locaux/evolution.toml")" "taux_piege = 0.5"

echo "Comparaison, note, suppression"
sleep 1.1
curl -s -b "$J" -o /dev/null -X POST "$U/evolution/lancer" -d 'mode=sim&generations=3&population=5'
attendre_fin; ID2=$(basename "$(dernier_run)" .json)
contient "comparaison : le changement de taux_piege apparaît" "$(curl -s -b "$J" "$U/runs/comparer?a=$ID1&b=$ID2")" "taux_piege"
curl -s -b "$J" -o /dev/null -X POST "$U/runs/note" --data-urlencode "run=$ID1" --data-urlencode "note=essai de note"
contient "note visible dans l'historique" "$(curl -s -b "$J" "$U/runs")" "essai de note"
curl -s -b "$J" -o /dev/null -X POST "$U/runs/supprimer" -d "run=$ID2"
verifie "run supprimé" "$(test -e "$T/donnees/evolution/$ID2.json" && echo oui || echo non)" "non"

echo "Réponses et mémorial"
verifie "réponse et testament enregistrés" "$(python3 -c "import json;a=json.load(open('$R1'))['generations'][0]['agents'][0];print('reponse' in a and 'testament' in a)")" "True"
contient "section réponses sur le tableau de bord" "$(curl -s -b "$J" "$U/?run=$ID1")" "Réponses des agents"
curl -s -b "$J" -o /dev/null -X POST "$U/memorial/depart" --data-urlencode $'memorial=Conseil A\n\nConseil B'
verifie "mémorial de départ enregistré (2 entrées)" "$(grep -c . "$T/agents-locaux/memorial.txt")" "2"
sleep 1.1
curl -s -b "$J" -o /dev/null -X POST "$U/evolution/lancer" -d 'mode=sim&generations=2&population=4&memorial=depart'
attendre_fin; R3=$(dernier_run); ID3=$(basename "$R3" .json)
verifie "run lancé avec le mémorial de départ" "$(python3 -c "import json;print(json.load(open('$R3'))['memorial_initial'])")" "['Conseil A', 'Conseil B']"
verifie "la 1re génération l'a reçu" "$(python3 -c "import json;print(json.load(open('$R3'))['generations'][0]['memorial_recu'])")" "['Conseil A', 'Conseil B']"
verifie "le mémorial transmis contient des testaments" "$(python3 -c "import json;print('«' in json.load(open('$R3'))['generations'][1]['memorial_recu'][-1])")" "True"
verifie "mutation refusée sur un run terminé" "$(curl -s -b "$J" -o /dev/null -w '%{redirect_url}' -X POST "$U/runs/memorial" -d "run=$ID3&memorial=x" | grep -c 'termin')" "1"
curl -s -b "$J" -o /dev/null -X POST "$U/memorial/depart" -d "depuis_run=$ID3"
verifie "mémorial final copié comme départ" "$(grep -c . "$T/agents-locaux/memorial.txt")" "$(python3 -c "import json;print(len(json.load(open('$R3'))['memorial']))")"

echo "Fichiers (mode expert)"
verifie "config cassée refusée" "$(curl -s -b "$J" -o /dev/null -w '%{redirect_url}' -X POST "$U/fichiers" --data-urlencode chemin=agents-locaux/evolution.toml --data-urlencode 'contenu=taille = = 1' | grep -c 'NON')" "1"
verifie "chemin hors arène refusé" "$(curl -s -b "$J" -o /dev/null -w '%{redirect_url}' "$U/fichiers?chemin=../etc/passwd" | grep -c 'refus')" "1"

echo; echo "$OK réussis, $KO échoués"
[ "$KO" -eq 0 ]
