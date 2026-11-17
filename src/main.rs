//! L'Arène des générations — serveur de l'interface (un seul jeu : l'évolution de Qwen en local).
//! Pages : tableau de bord, historique des runs, comparaison, paramètres, fichiers (mode expert).
//! Tout le texte affiché vient des gabarits de contenu/, relus à chaque requête.
use arene::evo::{self, ConfigEvo, ModeleSim, CHEMIN_CONFIG};
use arene::store::*;
use axum::{
    extract::{ConnectInfo, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};

type Params = HashMap<String, String>;
type St = State<Arc<App>>;
type Client = ConnectInfo<SocketAddr>;

const COOKIE: &str = "arene_admin";
const DOSSIER_RUNS: &str = "donnees/evolution";
const FICHIER_EN_COURS: &str = "donnees/evolution/.en-cours.toml";
/// Mémorial de départ réutilisable, modifiable par l'arbitre (une entrée par ligne).
const MEMORIAL_DEPART: &str = "agents-locaux/memorial.txt";

struct App {
    racine: PathBuf,
    verrou: Mutex<()>,
    /// run lancé depuis le site (un seul à la fois, pour ne pas saturer le GPU)
    evo: Mutex<Option<Processus>>,
    /// runs dont la synthèse par Qwen est en cours de rédaction
    syntheses: Mutex<std::collections::HashSet<String>>,
}

#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
#[serde(default)]
struct EnCours {
    run_id: String,
    pid: u32,
    debut: String,
    mode: String,
    gen: u32,
    pop: usize,
    question: String,
}

struct Processus {
    /// None si le run a été repris après un redémarrage du serveur (on ne garde alors que le pid)
    child: Option<Child>,
    info: EnCours,
}

// =================================================================== utilitaires
fn params(q: Params, corps: &str) -> Params {
    let mut p = q;
    if let Ok(m) = serde_urlencoded::from_str::<Vec<(String, String)>>(corps.trim()) {
        for (k, v) in m {
            p.insert(k, v);
        }
    }
    p
}

fn champ(p: &Params, cle: &str) -> String {
    p.get(cle).cloned().unwrap_or_default().trim().to_string()
}

fn texte(code: StatusCode, s: String) -> Response {
    (code, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], s).into_response()
}

fn vers(url: &str, info: &str) -> Response {
    if info.is_empty() {
        return Redirect::to(url).into_response();
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    let qs = serde_urlencoded::to_string([("info", info)]).unwrap_or_default();
    Redirect::to(&format!("{url}{sep}{qs}")).into_response()
}

fn cookie(h: &HeaderMap, nom: &str) -> Option<String> {
    h.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|c| {
        let (k, v) = c.trim().split_once('=')?;
        (k == nom).then(|| v.to_string())
    })
}

/// Une entrée par ligne non vide.
fn lignes(texte: &str) -> Vec<String> {
    texte.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
}

fn id_valide(id: &str) -> bool {
    !id.is_empty() && id.len() < 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn processus_vivant(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/cmdline")).map(|c| c.contains("evolution")).unwrap_or(false)
}

/// « 20261114-173446-bff9 » (heure UTC) -> « 14/11/2026 20:34 » (heure locale).
fn date_locale(id: &str) -> String {
    id.get(..15)
        .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y%m%d-%H%M%S").ok())
        .map(|n| n.and_utc().with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_else(|| id.to_string())
}

// =================================================================== App
impl App {
    fn p(&self, rel: &str) -> PathBuf {
        self.racine.join(rel)
    }

    fn config(&self) -> Result<ConfigEvo, Vec<String>> {
        evo::charger(&self.racine)
    }

    fn cle_admin(&self) -> String {
        fs::read_to_string(self.p("donnees/cle-admin.txt")).unwrap_or_default().trim().to_string()
    }

    /// Arbitre = cookie valide, ou (si activé) navigateur sur la machine elle-même.
    /// Une requête passée par un tunnel porte des en-têtes de proxy : elle n'est jamais considérée comme locale.
    fn est_admin(&self, h: &HeaderMap, client: SocketAddr) -> bool {
        let cle = self.cle_admin();
        if !cle.is_empty() && cookie(h, COOKIE).as_deref() == Some(cle.as_str()) {
            return true;
        }
        let locale = client.ip().is_loopback() && !h.contains_key("cf-connecting-ip") && !h.contains_key("x-forwarded-for");
        locale && self.config().map(|c| c.connexion_locale_automatique).unwrap_or(false)
    }

    /// Run en cours lancé depuis le site, ou None (un processus terminé est oublié).
    fn evo_actif(&self) -> Option<EnCours> {
        let mut g = l(&self.evo);
        let fini = match g.as_mut() {
            None => return None,
            Some(p) => match p.child.as_mut() {
                Some(c) => !matches!(c.try_wait(), Ok(None)),
                None => !processus_vivant(p.info.pid),
            },
        };
        if fini {
            *g = None;
            let _ = fs::remove_file(self.p(FICHIER_EN_COURS));
            return None;
        }
        g.as_ref().map(|p| p.info.clone())
    }

    fn chemin_run(&self, id: &str, ext: &str) -> PathBuf {
        self.p(&format!("{DOSSIER_RUNS}/{id}.{ext}"))
    }

    fn ids_runs(&self) -> Vec<String> {
        let mut ids: Vec<String> = fichiers(&self.p(DOSSIER_RUNS)).iter()
            .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
            .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
            .collect();
        ids.sort();
        ids.reverse();
        ids
    }

    /// Lit un run. Un run marqué « en cours » sans processus vivant depuis 5 min est affiché comme interrompu.
    fn lire_run(&self, id: &str, actif: &Option<EnCours>) -> Value {
        let chemin = self.chemin_run(id, "json");
        let mut run: Value = fs::read_to_string(&chemin).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
        if run.is_null() {
            return run;
        }
        let suivi_ici = actif.as_ref().map(|a| a.run_id == id).unwrap_or(false);
        let age = fs::metadata(&chemin).and_then(|m| m.modified()).ok()
            .and_then(|t| t.elapsed().ok()).map(|d| d.as_secs()).unwrap_or(0);
        if run["en_cours"].as_bool() == Some(true) && !suivi_ici && age > 300 {
            run["en_cours"] = json!(false);
            run["interrompu"] = json!(true);
        }
        run["note"] = json!(fs::read_to_string(self.chemin_run(id, "note.txt")).unwrap_or_default());
        run["memorial_en_attente"] = json!(fs::read_to_string(self.chemin_run(id, "memorial.txt")).ok());
        run["date_locale"] = json!(date_locale(id));
        run["synthese"] = json!(fs::read_to_string(self.chemin_run(id, "synthese.txt")).ok());
        run["podium_arbitre"] = fs::read_to_string(self.chemin_run(id, "podium.json")).ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Null);
        run
    }

    /// Résumé d'un run pour l'historique.
    fn resume(&self, id: &str, actif: &Option<EnCours>) -> Value {
        let r = self.lire_run(id, actif);
        let gens = r["generations"].as_array().cloned().unwrap_or_default();
        let der = gens.last().cloned().unwrap_or(Value::Null);
        let questions: Vec<&str> = gens.iter().filter_map(|g| g["enonce"].as_str()).collect();
        let imposee = questions.len() > 1 && questions.iter().all(|q| *q == questions[0]);
        json!({
            "id": id, "date": r["date_locale"], "mode": r["mode"], "modele": r["modele"],
            "faites": gens.len(), "prevues": r["generations_prevues"].as_u64().unwrap_or(gens.len() as u64),
            "population": r["population"], "extinctions": r["extinctions"],
            "survivants": der["survivants"], "difficulte": r["difficulte_actuelle"], "risque": der["risque_moyen"],
            "secondes": r["secondes"], "en_cours": r["en_cours"], "interrompu": r["interrompu"],
            "note": r["note"], "question_imposee": if imposee { json!(questions[0]) } else { Value::Null },
            "a_config": !r["config"].is_null(),
        })
    }

    fn rendre(&self, nom: &str, ctx: &Value) -> Response {
        let mut env = minijinja::Environment::new();
        env.set_loader(minijinja::path_loader(self.p("contenu")));
        let res = env.get_template(nom).and_then(|t| t.render(ctx));
        match res {
            Ok(s) => Html(s).into_response(),
            Err(e) => texte(StatusCode::INTERNAL_SERVER_ERROR, format!("Erreur dans le gabarit contenu/{nom} :\n{e:#}")),
        }
    }

    /// Contexte commun à toutes les pages.
    fn contexte(&self, page: &str, h: &HeaderMap, client: SocketAddr, q: &Params) -> Value {
        let cfg = self.config();
        json!({
            "page": page,
            "admin": self.est_admin(h, client),
            "actif": self.evo_actif(),
            "info": champ(q, "info"),
            "config": cfg.as_ref().ok(),
            "erreurs_config": cfg.err(),
            "moteur_present": self.chemin_moteur().exists(),
            "memorial_depart": self.memorial_depart(),
            "estimation": self.estimation(),
            "runs_choix": self.ids_runs().iter().take(30).map(|id| json!({ "id": id, "date": date_locale(id) })).collect::<Vec<_>>(),
        })
    }

    /// Durée moyenne d'une génération lors du dernier run réel (pour estimer le prochain lancement).
    fn estimation(&self) -> Value {
        for id in self.ids_runs() {
            let Some(r) = fs::read_to_string(self.chemin_run(&id, "json")).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()) else { continue };
            let gens = r["generations"].as_array().map(|a| a.len()).unwrap_or(0);
            if r["mode"] != "reel" || gens == 0 { continue; }
            let secondes = r["secondes"].as_u64().unwrap_or(0) as f64 / gens as f64;
            let population = r["population"].as_u64().unwrap_or(1).max(1) as f64;
            return json!({ "par_generation": secondes.round(), "par_agent": (secondes / population).round(),
                           "outils": r["config"]["outil_executer"].as_bool().unwrap_or(false) || r["config"]["outil_web"].as_bool().unwrap_or(false),
                           "date": date_locale(&id) });
        }
        Value::Null
    }

    fn memorial_depart(&self) -> Vec<String> {
        lignes(&fs::read_to_string(self.p(MEMORIAL_DEPART)).unwrap_or_default())
    }

    /// Mémorial final d'un run terminé.
    fn memorial_final(&self, id: &str) -> Vec<String> {
        fs::read_to_string(self.chemin_run(id, "json")).ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| v["memorial"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()))
            .unwrap_or_default()
    }

    fn chemin_moteur(&self) -> PathBuf {
        std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("evolution")))
            .filter(|p| p.exists())
            .unwrap_or_else(|| self.p("target/release/evolution"))
    }

    /// Fichiers modifiables en mode expert.
    fn chemin_editable(&self, rel: &str) -> Option<PathBuf> {
        let interdit = rel.is_empty() || rel.contains("..") || rel.starts_with('/') || rel.contains('\\')
            || rel.split('/').any(|s| s.starts_with('.'));
        let ok = ["agents-locaux/", "contenu/", "donnees/evolution/"].iter().any(|d| rel.starts_with(d));
        (!interdit && ok).then(|| self.p(rel))
    }

    fn liste_editables(&self) -> Vec<Value> {
        let mut v = Vec::new();
        for d in ["agents-locaux", "contenu", DOSSIER_RUNS] {
            for f in fichiers(&self.p(d)) {
                let rel = f.strip_prefix(&self.racine).unwrap_or(&f).to_string_lossy().to_string();
                let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ["toml", "txt", "html", "json", "log"].contains(&ext) && self.chemin_editable(&rel).is_some() {
                    v.push(json!({ "chemin": rel, "groupe": d }));
                }
            }
        }
        v
    }
}

// =================================================================== pages
async fn tableau(State(app): St, h: HeaderMap, ConnectInfo(c): Client, Query(q): Query<Params>) -> Response {
    let actif = app.evo_actif();
    let ids = app.ids_runs();
    let demande = champ(&q, "run");
    let id = if ids.contains(&demande) { demande } else { ids.first().cloned().unwrap_or_default() };
    let run = if id.is_empty() { Value::Null } else { app.lire_run(&id, &actif) };
    let mut ctx = app.contexte("tableau", &h, c, &q);
    ctx["run"] = run;
    ctx["id"] = json!(id);
    ctx["nb_runs"] = json!(ids.len());
    app.rendre("tableau.html", &ctx)
}

async fn page_resultats(State(app): St, h: HeaderMap, ConnectInfo(c): Client, Query(q): Query<Params>) -> Response {
    let actif = app.evo_actif();
    let ids = app.ids_runs();
    let demande = champ(&q, "run");
    let id = if ids.contains(&demande) { demande } else { ids.first().cloned().unwrap_or_default() };
    let run = if id.is_empty() { Value::Null } else { app.lire_run(&id, &actif) };
    let mut ctx = app.contexte("resultats", &h, c, &q);
    ctx["synthese_en_cours"] = json!(l(&app.syntheses).contains(&id));
    ctx["run"] = run;
    ctx["id"] = json!(id);
    app.rendre("resultats.html", &ctx)
}

async fn page_runs(State(app): St, h: HeaderMap, ConnectInfo(c): Client, Query(q): Query<Params>) -> Response {
    let actif = app.evo_actif();
    let runs: Vec<Value> = app.ids_runs().iter().map(|id| app.resume(id, &actif)).collect();
    let mut ctx = app.contexte("runs", &h, c, &q);
    ctx["runs"] = json!(runs);
    app.rendre("runs.html", &ctx)
}

async fn page_comparer(State(app): St, h: HeaderMap, ConnectInfo(c): Client, Query(q): Query<Params>) -> Response {
    let (a, b) = (champ(&q, "a"), champ(&q, "b"));
    let ids = app.ids_runs();
    if !ids.contains(&a) || !ids.contains(&b) || a == b {
        return vers("/runs", "Coche exactement deux runs différents pour les comparer.");
    }
    let actif = app.evo_actif();
    let (ra, rb) = (app.lire_run(&a, &actif), app.lire_run(&b, &actif));
    // différences de paramètres : ce qui a été « muté » entre les deux runs
    let mut diff = Vec::new();
    if let (Some(ca), Some(cb)) = (ra["config"].as_object(), rb["config"].as_object()) {
        let mut cles: Vec<&String> = ca.keys().chain(cb.keys()).collect();
        cles.sort();
        cles.dedup();
        for k in cles {
            let (va, vb) = (ca.get(k).cloned().unwrap_or(Value::Null), cb.get(k).cloned().unwrap_or(Value::Null));
            if va != vb {
                let aff = |v: &Value| match v { Value::String(s) => s.clone(), autre => autre.to_string() };
                diff.push(json!({ "cle": k, "a": aff(&va), "b": aff(&vb) }));
            }
        }
    }
    let mut ctx = app.contexte("runs", &h, c, &q);
    ctx["a"] = json!({ "id": a, "resume": app.resume(&a, &actif) });
    ctx["b"] = json!({ "id": b, "resume": app.resume(&b, &actif) });
    ctx["runs"] = json!([ra, rb]);
    ctx["diff"] = json!(diff);
    ctx["configs_presentes"] = json!(!ra["config"].is_null() && !rb["config"].is_null());
    app.rendre("comparer.html", &ctx)
}

async fn page_parametres(State(app): St, h: HeaderMap, ConnectInfo(c): Client, Query(q): Query<Params>) -> Response {
    let mut ctx = app.contexte("parametres", &h, c, &q);
    if let Ok(cfg) = app.config() {
        ctx["form"] = form_depuis_config(&cfg);
        ctx["etat_outils"] = etat_outils(&cfg);
    }
    app.rendre("parametres.html", &ctx)
}

async fn page_fichiers(State(app): St, h: HeaderMap, ConnectInfo(c): Client, Query(q): Query<Params>) -> Response {
    if !app.est_admin(&h, c) {
        return vers("/connexion", "Connecte-toi pour accéder aux fichiers.");
    }
    let mut ctx = app.contexte("fichiers", &h, c, &q);
    ctx["fichiers"] = json!(app.liste_editables());
    let rel = champ(&q, "chemin");
    if !rel.is_empty() {
        let Some(chemin) = app.chemin_editable(&rel) else {
            return vers("/fichiers", "Chemin refusé.");
        };
        ctx["chemin"] = json!(rel);
        ctx["contenu"] = json!(fs::read_to_string(&chemin).unwrap_or_default());
        ctx["existe"] = json!(chemin.exists());
    }
    app.rendre("fichiers.html", &ctx)
}

async fn page_connexion(State(app): St, h: HeaderMap, ConnectInfo(c): Client, Query(q): Query<Params>) -> Response {
    if q.contains_key("cle") {
        return connecter(&app, &champ(&q, "cle"));
    }
    let ctx = app.contexte("connexion", &h, c, &q);
    app.rendre("connexion.html", &ctx)
}

// =================================================================== API (mise à jour en direct)
async fn api_evolution(State(app): St, Query(q): Query<Params>) -> Json<Value> {
    let actif = app.evo_actif();
    let ids = app.ids_runs();
    let demande = champ(&q, "run");
    let id = if ids.contains(&demande) { demande } else { ids.first().cloned().unwrap_or_default() };
    let run = if id.is_empty() { Value::Null } else { app.lire_run(&id, &actif) };
    Json(json!({ "id": id, "run": run, "actif": actif }))
}

/// Fonctions de graphiques partagées (fichier modifiable : contenu/graphiques.js).
async fn graphiques_js(State(app): St) -> Response {
    match fs::read_to_string(app.p("contenu/graphiques.js")) {
        Ok(s) => ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], s).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

// =================================================================== actions de l'arbitre
fn connecter(app: &App, cle: &str) -> Response {
    if cle.is_empty() || cle != app.cle_admin() {
        return vers("/connexion", "Clé incorrecte.");
    }
    let c = format!("{COOKIE}={cle}; Path=/; HttpOnly; SameSite=Strict; Max-Age=2592000");
    ([(header::SET_COOKIE, c)], Redirect::to("/")).into_response()
}

async fn post_connexion(State(app): St, corps: String) -> Response {
    connecter(&app, &champ(&params(Params::new(), &corps), "cle"))
}

async fn deconnexion() -> Response {
    let c = format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0");
    ([(header::SET_COOKIE, c)], Redirect::to("/")).into_response()
}

async fn lancer(State(app): St, h: HeaderMap, ConnectInfo(c): Client, corps: String) -> Response {
    if !app.est_admin(&h, c) {
        return vers("/connexion", "Connecte-toi pour lancer une évolution.");
    }
    if let Err(e) = app.config() {
        return vers("/parametres", &format!("Paramètres invalides, corrige-les d'abord : {}", e.join(" ; ")));
    }
    let _g = l(&app.verrou);
    if app.evo_actif().is_some() {
        return vers("/", "Une évolution tourne déjà : arrête-la avant d'en lancer une autre.");
    }
    let moteur = app.chemin_moteur();
    if !moteur.exists() {
        return vers("/", "Moteur absent : lance « cargo build --release » dans le dossier de l'arène.");
    }
    let p = params(Params::new(), &corps);
    let mode = if champ(&p, "mode") == "sim" { "sim" } else { "reel" };
    let gen = champ(&p, "generations").parse::<u32>().unwrap_or(10).clamp(1, 500);
    let pop = champ(&p, "population").parse::<usize>().unwrap_or(6).clamp(1, 100);
    let par = champ(&p, "parallele").parse::<usize>().unwrap_or(2).clamp(1, 16);
    let question = champ(&p, "question");
    let reponse = champ(&p, "reponse");
    let piege = p.contains_key("piege");
    if !question.is_empty() && !piege && reponse.is_empty() {
        return vers("/", "Question imposée : donne la réponse attendue, ou coche « piège ».");
    }
    let memorial_initial: Vec<String> = match champ(&p, "memorial").as_str() {
        "depart" => app.memorial_depart(),
        src if src.starts_with("run:") && id_valide(&src[4..]) => app.memorial_final(&src[4..]),
        _ => Vec::new(),
    };
    let run_id = format!("{}-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"), &nouveau_jeton()[..4]);
    let _ = fs::create_dir_all(app.p(DOSSIER_RUNS));
    let sorties = fs::File::create(app.chemin_run(&run_id, "log")).and_then(|f| Ok((f.try_clone()?, f)));
    let (sortie, erreurs) = match sorties {
        Ok(x) => x,
        Err(e) => return vers("/", &format!("Impossible de créer le journal : {e}")),
    };
    let lance = Command::new(&moteur)
        .current_dir(&app.racine)
        .args([mode, &gen.to_string(), &pop.to_string(), &par.to_string()])
        .env("ARENE_RUN_ID", &run_id)
        .env("ARENE_QUESTION", &question)
        .env("ARENE_REPONSE", &reponse)
        .env("ARENE_PIEGE", if piege { "1" } else { "0" })
        .env("ARENE_MEMORIAL", memorial_initial.join("\n"))
        .stdout(Stdio::from(sortie))
        .stderr(Stdio::from(erreurs))
        .spawn();
    match lance {
        Ok(child) => {
            let info = EnCours { run_id: run_id.clone(), pid: child.id(), debut: maintenant(), mode: mode.into(), gen, pop, question };
            let _ = ecrire_toml(&app.p(FICHIER_EN_COURS), &info);
            *l(&app.evo) = Some(Processus { child: Some(child), info });
            git_commit(format!("Évolution lancée : {mode}, {gen} générations × {pop} agents (run {run_id})"));
            vers(&format!("/?run={run_id}"), "")
        }
        Err(e) => vers("/", &format!("Échec du lancement : {e}")),
    }
}

async fn arreter(State(app): St, h: HeaderMap, ConnectInfo(c): Client) -> Response {
    if !app.est_admin(&h, c) {
        return vers("/connexion", "Connecte-toi pour arrêter une évolution.");
    }
    let processus = l(&app.evo).take();
    let Some(mut p) = processus else {
        return vers("/", "Aucune évolution en cours.");
    };
    match p.child.as_mut() {
        Some(ch) => {
            let _ = ch.kill();
            let _ = ch.wait();
        }
        None => {
            let _ = Command::new("kill").arg(p.info.pid.to_string()).output();
        }
    }
    let _ = fs::remove_file(app.p(FICHIER_EN_COURS));
    // le moteur a été coupé avant d'écrire sa fin : on marque le journal comme interrompu
    let chemin = app.chemin_run(&p.info.run_id, "json");
    if let Some(mut run) = fs::read_to_string(&chemin).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
        run["en_cours"] = json!(false);
        run["interrompu"] = json!(true);
        run["question_en_cours"] = json!("");
        let _ = ecrire_atomique(&chemin, serde_json::to_string_pretty(&run).unwrap_or_default().as_bytes());
    }
    let modele = app.config().map(|c| c.ollama_modele).unwrap_or_else(|_| "qwen3:4b".into());
    let _ = Command::new("ollama").args(["stop", &modele]).output();
    git_commit(format!("Évolution {} arrêtée par l'arbitre", p.info.run_id));
    vers(&format!("/?run={}", p.info.run_id), "Évolution arrêtée, modèle déchargé.")
}

async fn noter(State(app): St, h: HeaderMap, ConnectInfo(c): Client, corps: String) -> Response {
    if !app.est_admin(&h, c) {
        return vers("/connexion", "Connecte-toi pour annoter.");
    }
    let p = params(Params::new(), &corps);
    let id = champ(&p, "run");
    if !id_valide(&id) || !app.chemin_run(&id, "json").exists() {
        return vers("/runs", "Run introuvable.");
    }
    let note = p.get("note").cloned().unwrap_or_default().replace("\r\n", "\n").trim().to_string();
    let chemin = app.chemin_run(&id, "note.txt");
    let res = if note.is_empty() { fs::remove_file(&chemin).or(Ok(())) } else { ecrire_atomique(&chemin, note.as_bytes()).map_err(std::io::Error::other) };
    if let Err(e) = res {
        return vers("/runs", &format!("Erreur : {e}"));
    }
    git_commit(format!("Note du run {id} modifiée"));
    let retour = champ(&p, "retour");
    vers(if retour == "tableau" { "/" } else { "/runs" }, "Note enregistrée.")
}

/// Mutation du mémorial pendant un run : appliquée par le moteur au début de la génération suivante.
async fn muter_memorial(State(app): St, h: HeaderMap, ConnectInfo(c): Client, corps: String) -> Response {
    if !app.est_admin(&h, c) {
        return vers("/connexion", "Connecte-toi pour modifier le mémorial.");
    }
    let p = params(Params::new(), &corps);
    let id = champ(&p, "run");
    let retour = format!("/?run={id}#memorial");
    if !app.evo_actif().map(|a| a.run_id == id).unwrap_or(false) {
        return vers(&retour, "Ce run est terminé : son mémorial ne peut plus agir. Utilise-le plutôt comme mémorial de départ d'un nouveau run.");
    }
    let texte = lignes(&p.get("memorial").cloned().unwrap_or_default()).join("\n");
    if let Err(e) = ecrire_atomique(&app.chemin_run(&id, "memorial.txt"), texte.as_bytes()) {
        return vers(&retour, &format!("Erreur : {e}"));
    }
    git_commit(format!("Arbitre : mutation du mémorial du run {id}"));
    vers(&retour, "Mémorial modifié : il sera transmis à la prochaine génération.")
}

/// Synthèse des réponses d'un run : texte réécrit par l'arbitre, ou rédigé par Qwen en arrière-plan.
async fn synthese(State(app): St, h: HeaderMap, ConnectInfo(c): Client, corps: String) -> Response {
    if !app.est_admin(&h, c) {
        return vers("/connexion", "Connecte-toi pour modifier la synthèse.");
    }
    let p = params(Params::new(), &corps);
    let id = champ(&p, "run");
    if !id_valide(&id) || !app.chemin_run(&id, "json").exists() {
        return vers("/resultats", "Run introuvable.");
    }
    let retour = format!("/resultats?run={id}#synthese");
    let chemin = app.chemin_run(&id, "synthese.txt");
    if champ(&p, "action") != "qwen" {
        let texte = p.get("synthese").cloned().unwrap_or_default().replace("\r\n", "\n").trim().to_string();
        let res = if texte.is_empty() { fs::remove_file(&chemin).or(Ok(())) } else { ecrire_atomique(&chemin, texte.as_bytes()).map_err(std::io::Error::other) };
        if let Err(e) = res {
            return vers(&retour, &format!("Erreur : {e}"));
        }
        git_commit(format!("Arbitre : synthèse du run {id} modifiée"));
        return vers(&retour, "Synthèse enregistrée.");
    }
    let cfg = match app.config() {
        Ok(c) => c,
        Err(_) => return vers(&retour, "Paramètres invalides : corrige-les avant de demander une synthèse."),
    };
    let run = app.lire_run(&id, &None);
    let Some(prompt) = prompt_synthese(&run) else {
        return vers(&retour, "Aucune réponse à résumer dans ce run.");
    };
    if !l(&app.syntheses).insert(id.clone()) {
        return vers(&retour, "Une synthèse est déjà en cours de rédaction pour ce run.");
    }
    let app2 = app.clone();
    tokio::spawn(async move {
        let texte = match rediger_synthese(&cfg, &prompt).await {
            Ok(t) => t,
            Err(e) => format!("(La synthèse par Qwen a échoué : {e})"),
        };
        let _ = ecrire_atomique(&app2.chemin_run(&id, "synthese.txt"), texte.as_bytes());
        git_commit(format!("Synthèse du run {id} rédigée par Qwen"));
        l(&app2.syntheses).remove(&id);
    });
    let attente = if app.evo_actif().is_some() { " Une évolution tourne : Qwen la traitera entre deux agents, compte quelques minutes." } else { "" };
    vers(&retour, &format!("Qwen rédige la synthèse (environ une minute).{attente} La page se mettra à jour toute seule."))
}

/// Toutes les réponses d'un run, regroupées par question, pour la synthèse. None s'il n'y en a aucune.
fn prompt_synthese(run: &Value) -> Option<String> {
    let mut blocs: Vec<(String, String, Vec<String>)> = Vec::new();
    for g in run["generations"].as_array().into_iter().flatten() {
        let enonce = g["enonce"].as_str().unwrap_or("").to_string();
        let attendu = if g["genre"] == "PIÈGE" { "REFUS (question piège)".to_string() } else { g["attendu"].as_str().unwrap_or("").to_string() };
        let i = match blocs.iter().position(|b| b.0 == enonce) {
            Some(i) => i,
            None => { blocs.push((enonce, attendu, Vec::new())); blocs.len() - 1 }
        };
        for a in g["agents"].as_array().into_iter().flatten() {
            let choix = a["choix"].as_str().unwrap_or("?");
            let rep: String = a["reponse"].as_str().unwrap_or("").chars().take(400).collect();
            let juste = if a["correct"] == true { "juste" } else { "pas juste" };
            blocs[i].2.push(format!("- G{} agent #{} : {choix}, {juste} — {}", g["gen"], a["id"], rep.replace('\n', " ")));
        }
    }
    if blocs.iter().all(|b| b.2.is_empty()) {
        return None;
    }
    let mut texte = String::new();
    for (enonce, attendu, reps) in &blocs {
        texte.push_str(&format!("\nQUESTION : {enonce}\nRéponse attendue par le juge : {}\n", if attendu.is_empty() { "(non précisée)" } else { attendu }));
        // au-delà de 60 réponses par question, on garde les dernières (les plus évoluées)
        for r in reps.iter().skip(reps.len().saturating_sub(60)) {
            texte.push_str(r);
            texte.push('\n');
        }
    }
    let texte: String = texte.chars().take(24_000).collect();
    Some(format!("Voici les réponses données par une population d'agents IA, génération après génération.\n{texte}\n\
        Rédige en français une synthèse pour l'arbitre du jeu, en 150 à 300 mots :\n\
        1. ce que la majorité des agents a répondu (le consensus), et si c'est juste ;\n\
        2. les réponses minoritaires ou originales qui méritent d'être signalées ;\n\
        3. comment les réponses ont évolué au fil des générations.\n\
        Cite les numéros d'agents. N'invente rien qui ne figure pas dans les réponses."))
}

async fn rediger_synthese(cfg: &ConfigEvo, prompt: &str) -> Result<String, String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(900)).build().map_err(|e| e.to_string())?;
    let corps = json!({
        "model": cfg.ollama_modele, "stream": false, "keep_alive": cfg.keep_alive, "think": false,
        "options": { "num_ctx": cfg.num_ctx, "temperature": 0.3 },
        "messages": [{ "role": "user", "content": prompt }],
    });
    let url = format!("{}/api/chat", cfg.ollama_hote.trim_end_matches('/'));
    let r: Value = client.post(url).json(&corps).send().await.map_err(|e| format!("Ollama injoignable ({e})"))?
        .json().await.map_err(|e| e.to_string())?;
    if let Some(e) = r["error"].as_str() {
        return Err(e.to_string());
    }
    let t = r["message"]["content"].as_str().unwrap_or("").trim().to_string();
    if t.is_empty() { Err("réponse vide".into()) } else { Ok(t) }
}

/// Podium désigné par l'arbitre (sinon le classement automatique s'applique).
async fn podium(State(app): St, h: HeaderMap, ConnectInfo(c): Client, corps: String) -> Response {
    if !app.est_admin(&h, c) {
        return vers("/connexion", "Connecte-toi pour désigner les vainqueurs.");
    }
    let p = params(Params::new(), &corps);
    let id = champ(&p, "run");
    if !id_valide(&id) || !app.chemin_run(&id, "json").exists() {
        return vers("/resultats", "Run introuvable.");
    }
    let retour = format!("/resultats?run={id}#podium");
    let chemin = app.chemin_run(&id, "podium.json");
    if champ(&p, "action") == "auto" {
        let _ = fs::remove_file(&chemin);
        git_commit(format!("Arbitre : podium automatique rétabli pour le run {id}"));
        return vers(&retour, "Podium automatique rétabli.");
    }
    let places: Vec<u64> = ["p1", "p2", "p3"].iter().filter_map(|k| champ(&p, k).parse().ok()).collect();
    let mut uniques = places.clone();
    uniques.sort();
    uniques.dedup();
    if places.is_empty() || uniques.len() != places.len() {
        return vers(&retour, "Choisis au moins un vainqueur, et trois agents différents.");
    }
    let motif = champ(&p, "motif");
    let v = json!({ "places": places, "motif": motif, "date": maintenant() });
    if let Err(e) = ecrire_atomique(&chemin, serde_json::to_string_pretty(&v).unwrap_or_default().as_bytes()) {
        return vers(&retour, &format!("Erreur : {e}"));
    }
    git_commit(format!("Arbitre : podium du run {id} désigné"));
    vers(&retour, "Podium enregistré.")
}

/// Mémorial de départ (réutilisable au lancement) : texte saisi ou copie du mémorial final d'un run.
async fn memorial_depart(State(app): St, h: HeaderMap, ConnectInfo(c): Client, corps: String) -> Response {
    if !app.est_admin(&h, c) {
        return vers("/connexion", "Connecte-toi pour modifier le mémorial de départ.");
    }
    let p = params(Params::new(), &corps);
    let depuis = champ(&p, "depuis_run");
    let (entrees, retour) = if id_valide(&depuis) {
        (app.memorial_final(&depuis), format!("/?run={depuis}#memorial"))
    } else {
        (lignes(&p.get("memorial").cloned().unwrap_or_default()), "/parametres#memorial".to_string())
    };
    if let Err(e) = ecrire_atomique(&app.p(MEMORIAL_DEPART), entrees.join("\n").as_bytes()) {
        return vers(&retour, &format!("Erreur : {e}"));
    }
    git_commit(format!("Arbitre : mémorial de départ modifié ({} entrées)", entrees.len()));
    vers(&retour, &format!("Mémorial de départ enregistré ({} entrées). Choisis-le au prochain lancement.", entrees.len()))
}

async fn supprimer(State(app): St, h: HeaderMap, ConnectInfo(c): Client, corps: String) -> Response {
    if !app.est_admin(&h, c) {
        return vers("/connexion", "Connecte-toi pour supprimer un run.");
    }
    let id = champ(&params(Params::new(), &corps), "run");
    if !id_valide(&id) || !app.chemin_run(&id, "json").exists() {
        return vers("/runs", "Run introuvable.");
    }
    if app.evo_actif().map(|a| a.run_id == id).unwrap_or(false) {
        return vers("/runs", "Ce run est en cours : arrête-le avant de le supprimer.");
    }
    for ext in ["json", "log", "note.txt", "memorial.txt"] {
        let _ = fs::remove_file(app.chemin_run(&id, ext));
    }
    git_commit(format!("Run {id} supprimé (récupérable dans l'historique Git)"));
    vers("/runs", &format!("Run du {} supprimé.", date_locale(&id)))
}

/// État des services dont dépendent les outils (affiché dans Paramètres).
fn etat_outils(cfg: &ConfigEvo) -> Value {
    let image = Command::new("docker").args(["image", "inspect", "-f", ".", &cfg.image_bac_a_sable]).output()
        .map(|o| o.status.success()).unwrap_or(false);
    let searxng = reqwest::Url::parse(&cfg.searxng_url).ok()
        .and_then(|u| Some(format!("{}:{}", u.host_str()?, u.port_or_known_default()?)))
        .and_then(|a| a.parse::<SocketAddr>().ok().or_else(|| std::net::ToSocketAddrs::to_socket_addrs(&a).ok()?.next()))
        .map(|a| std::net::TcpStream::connect_timeout(&a, std::time::Duration::from_millis(500)).is_ok())
        .unwrap_or(false);
    json!({ "image": image, "searxng": searxng })
}

// ------------------------------------------------------------------ paramètres (formulaire)
fn form_depuis_config(c: &ConfigEvo) -> Value {
    let mut v = serde_json::to_value(c).unwrap_or(Value::Null);
    v["principes_txt"] = json!(c.principes.join("\n"));
    v["pieges_txt"] = json!(c.pieges.join("\n"));
    v["modeles_txt"] = json!(c.modeles.iter().map(|m| format!("{} = {}", m.nom, m.competence)).collect::<Vec<_>>().join("\n"));
    v
}

/// Construit une config à partir du formulaire (les champs absents gardent leur valeur actuelle).
fn config_depuis_form(base: &ConfigEvo, p: &Params) -> Result<ConfigEvo, Vec<String>> {
    let mut c = base.clone();
    let mut erreurs = Vec::new();
    macro_rules! nombre {
        ($champ:ident) => {
            if let Some(s) = p.get(stringify!($champ)) {
                match s.trim().replace(',', ".").parse() {
                    Ok(v) => c.$champ = v,
                    Err(_) => erreurs.push(format!("{} : « {} » n'est pas un nombre valide", stringify!($champ), s.trim())),
                }
            }
        };
    }
    macro_rules! chaine {
        ($champ:ident) => {
            if let Some(s) = p.get(stringify!($champ)) {
                c.$champ = s.replace("\r\n", "\n").trim().to_string();
            }
        };
    }
    nombre!(taille); nombre!(generations); nombre!(duree_max_generation_min); nombre!(survivants); nombre!(parallele); nombre!(memorial_max);
    nombre!(difficulte_depart); nombre!(cible_min); nombre!(cible_max); nombre!(pas); nombre!(seuil_multiplication);
    nombre!(operande_max_base); nombre!(operande_max_ampleur); nombre!(taux_piege);
    nombre!(p_risque); nombre!(p_temperature); nombre!(p_principe); nombre!(p_modele); nombre!(p_croisement);
    nombre!(num_predict); nombre!(budget_reflexion); nombre!(num_ctx); nombre!(outils_max_etapes); nombre!(outil_delai_secondes); nombre!(p_outils);
    chaine!(ollama_modele); chaine!(ollama_hote); chaine!(keep_alive); chaine!(systeme); chaine!(connaissances); chaine!(searxng_url); chaine!(image_bac_a_sable); chaine!(adresse_serveur);
    if p.contains_key("formulaire") {
        c.think = p.contains_key("think");
        c.connexion_locale_automatique = p.contains_key("connexion_locale_automatique");
        c.outil_executer = p.contains_key("outil_executer");
        c.outil_web = p.contains_key("outil_web");
    }
    let lignes = |cle: &str| -> Option<Vec<String>> {
        p.get(cle).map(|s| s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
    };
    if let Some(v) = lignes("principes_txt") { c.principes = v; }
    if let Some(v) = lignes("pieges_txt") { c.pieges = v; }
    if let Some(v) = lignes("modeles_txt") {
        let mut modeles = Vec::new();
        for l in v {
            let (nom, comp) = l.split_once('=').or_else(|| l.split_once(';')).unwrap_or((l.as_str(), "0.5"));
            match comp.trim().replace(',', ".").parse::<f64>() {
                Ok(x) => modeles.push(ModeleSim { nom: nom.trim().to_string(), competence: x }),
                Err(_) => erreurs.push(format!("modèle « {l} » : écris « nom = compétence », par exemple « qwen3:4b = 0.45 »")),
            }
        }
        c.modeles = modeles;
    }
    erreurs.extend(c.valider());
    if erreurs.is_empty() { Ok(c) } else { Err(erreurs) }
}

async fn post_parametres(State(app): St, h: HeaderMap, ConnectInfo(cl): Client, corps: String) -> Response {
    if !app.est_admin(&h, cl) {
        return vers("/connexion", "Connecte-toi pour modifier les paramètres.");
    }
    let p = params(Params::new(), &corps);
    let base = app.config().unwrap_or_default();
    match config_depuis_form(&base, &p) {
        Ok(nouvelle) => {
            let (av, ap) = (serde_json::to_value(&base).unwrap_or_default(), serde_json::to_value(&nouvelle).unwrap_or_default());
            let changes: Vec<String> = ap.as_object().map(|o| o.keys().filter(|k| av.get(k.as_str()) != ap.get(k.as_str())).cloned().collect()).unwrap_or_default();
            if changes.is_empty() {
                return vers("/parametres", "Aucun changement.");
            }
            if let Err(e) = ecrire_atomique(&app.p(CHEMIN_CONFIG), nouvelle.vers_toml().as_bytes()) {
                return vers("/parametres", &format!("Erreur d'écriture : {e}"));
            }
            git_commit(format!("Paramètres modifiés : {}", changes.join(", ")));
            let redemarrer = if changes.iter().any(|k| k == "adresse_serveur") { " L'adresse du serveur ne change qu'au prochain redémarrage." } else { "" };
            vers("/parametres", &format!("Enregistré ({}). Pris en compte au prochain lancement.{redemarrer}", changes.join(", ")))
        }
        Err(erreurs) => {
            // on réaffiche le formulaire avec ce qui a été saisi et la liste des erreurs, sans rien enregistrer
            let mut ctx = app.contexte("parametres", &h, cl, &Params::new());
            let mut form = form_depuis_config(&base);
            for (k, v) in &p {
                form[k.as_str()] = json!(v);
            }
            form["think"] = json!(p.contains_key("think"));
            form["connexion_locale_automatique"] = json!(p.contains_key("connexion_locale_automatique"));
            form["outil_executer"] = json!(p.contains_key("outil_executer"));
            form["outil_web"] = json!(p.contains_key("outil_web"));
            ctx["form"] = form;
            ctx["etat_outils"] = etat_outils(&base);
            ctx["erreurs_form"] = json!(erreurs);
            app.rendre("parametres.html", &ctx)
        }
    }
}

// ------------------------------------------------------------------ fichiers (mode expert)
async fn post_fichiers(State(app): St, h: HeaderMap, ConnectInfo(c): Client, corps: String) -> Response {
    if !app.est_admin(&h, c) {
        return vers("/connexion", "Connecte-toi pour modifier les fichiers.");
    }
    let p = params(Params::new(), &corps);
    let rel = champ(&p, "chemin");
    let page = format!("/fichiers?{}", serde_urlencoded::to_string([("chemin", rel.as_str())]).unwrap_or_default());
    let Some(chemin) = app.chemin_editable(&rel) else {
        return vers("/fichiers", "Chemin refusé.");
    };
    if let Some(a) = app.evo_actif() {
        if rel.starts_with(&format!("{DOSSIER_RUNS}/{}", a.run_id)) {
            return vers(&page, "Ce run est en cours : le moteur écraserait ta modification. Attends la fin ou arrête-le.");
        }
    }
    let contenu = p.get("contenu").cloned().unwrap_or_default().replace("\r\n", "\n");
    if rel == CHEMIN_CONFIG {
        if let Err(e) = ConfigEvo::depuis_toml(&contenu) {
            return vers(&page, &format!("NON enregistré : {}", e.join(" ; ")));
        }
    } else if rel.ends_with(".toml") {
        if let Err(e) = toml::from_str::<toml::Value>(&contenu) {
            return vers(&page, &format!("NON enregistré, TOML invalide : {}", e.to_string().replace('\n', " ")));
        }
    } else if rel.ends_with(".json") {
        if let Err(e) = serde_json::from_str::<Value>(&contenu) {
            return vers(&page, &format!("NON enregistré, JSON invalide : {e}"));
        }
    }
    if let Err(e) = ecrire_atomique(&chemin, contenu.as_bytes()) {
        return vers(&page, &format!("Erreur : {e}"));
    }
    git_commit(format!("Arbitre : modifie {rel}"));
    vers(&page, "Enregistré.")
}

// =================================================================== démarrage
fn initialiser(racine: &Path) -> Option<Processus> {
    let _ = fs::create_dir_all(racine.join(DOSSIER_RUNS));
    let cle = racine.join("donnees/cle-admin.txt");
    if !cle.exists() {
        let _ = ecrire_atomique(&cle, nouveau_jeton().as_bytes());
    }
    git_demarrer(racine);
    // reprend un run lancé avant un redémarrage du serveur, s'il tourne encore
    let info: EnCours = lire_toml(&racine.join(FICHIER_EN_COURS)).unwrap_or_default();
    if info.pid != 0 && processus_vivant(info.pid) {
        println!("Run en cours repris : {} (pid {})", info.run_id, info.pid);
        Some(Processus { child: None, info })
    } else {
        let _ = fs::remove_file(racine.join(FICHIER_EN_COURS));
        None
    }
}

#[tokio::main]
async fn main() {
    let racine = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap());
    let racine = racine.canonicalize().unwrap_or(racine);
    let repris = initialiser(&racine);
    let cfg = match evo::charger(&racine) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{CHEMIN_CONFIG} invalide :\n  - {}\nL'interface démarre avec l'adresse par défaut ; corrige les paramètres depuis /parametres.", e.join("\n  - "));
            ConfigEvo::default()
        }
    };
    let app = Arc::new(App { racine, verrou: Mutex::new(()), evo: Mutex::new(repris), syntheses: Mutex::new(Default::default()) });

    let routes = Router::new()
        .route("/", get(tableau))
        .route("/runs", get(page_runs))
        .route("/runs/comparer", get(page_comparer))
        .route("/runs/note", axum::routing::post(noter))
        .route("/runs/supprimer", axum::routing::post(supprimer))
        .route("/resultats", get(page_resultats))
        .route("/resultats/synthese", axum::routing::post(synthese))
        .route("/resultats/podium", axum::routing::post(podium))
        .route("/runs/memorial", axum::routing::post(muter_memorial))
        .route("/memorial/depart", axum::routing::post(memorial_depart))
        .route("/parametres", get(page_parametres).post(post_parametres))
        .route("/fichiers", get(page_fichiers).post(post_fichiers))
        .route("/connexion", get(page_connexion).post(post_connexion))
        .route("/deconnexion", get(deconnexion))
        .route("/evolution/lancer", axum::routing::post(lancer))
        .route("/evolution/arreter", axum::routing::post(arreter))
        .route("/api/evolution", get(api_evolution))
        .route("/graphiques.js", get(graphiques_js))
        // anciennes adresses
        .route("/evolution", get(|| async { Redirect::permanent("/") }))
        .route("/admin", get(|| async { Redirect::permanent("/parametres") }))
        .with_state(app.clone());

    let adresse = cfg.adresse_serveur.clone();
    let listener = match tokio::net::TcpListener::bind(&adresse).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Impossible d'écouter sur {adresse} : {e} (un autre serveur tourne déjà ?)");
            std::process::exit(1);
        }
    };
    println!("Arène : http://{adresse}");
    println!("Dossier : {}", app.racine.display());
    if cfg.connexion_locale_automatique {
        println!("Connexion automatique depuis cette machine (désactivable dans /parametres).");
    } else {
        println!("Connexion : clé dans donnees/cle-admin.txt");
    }
    axum::serve(listener, routes.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulaire_vers_config() {
        let base = ConfigEvo::default();
        let mut p = Params::new();
        p.insert("formulaire".into(), "1".into());
        p.insert("taille".into(), "8".into());
        p.insert("taux_piege".into(), "0,2".into());
        p.insert("principes_txt".into(), "Un\n\nDeux\r\nTrois".into());
        p.insert("modeles_txt".into(), "a = 0.4\nb ; 0,7".into());
        let c = config_depuis_form(&base, &p).unwrap();
        assert_eq!(c.taille, 8);
        assert!((c.taux_piege - 0.2).abs() < 1e-9);
        assert_eq!(c.principes, vec!["Un", "Deux", "Trois"]);
        assert_eq!(c.modeles[1], ModeleSim { nom: "b".into(), competence: 0.7 });
        assert!(!c.think, "case non cochée = désactivé");
    }

    #[test]
    fn formulaire_incoherent_refuse() {
        let mut p = Params::new();
        p.insert("survivants".into(), "50".into());
        p.insert("taille".into(), "abc".into());
        let e = config_depuis_form(&ConfigEvo::default(), &p).unwrap_err();
        assert!(e.iter().any(|x| x.contains("taille")));
        assert!(e.iter().any(|x| x.contains("survivants")));
    }

    #[test]
    fn date_et_identifiants() {
        assert!(date_locale("20261114-173446-bff9").contains("2026"));
        assert!(id_valide("20261114-173446-bff9"));
        assert!(!id_valide("../cle-admin"));
    }
}
