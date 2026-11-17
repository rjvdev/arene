//! Configuration de l'évolution, partagée par le serveur (formulaire, validation) et le moteur.
//! Un seul fichier : agents-locaux/evolution.toml, modifiable par l'arbitre.
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CHEMIN_CONFIG: &str = "agents-locaux/evolution.toml";

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ModeleSim {
    pub nom: String,
    /// compétence simulée 0..1 (mode « sim » ; en « réel » tous les agents appellent `ollama_modele`)
    pub competence: f64,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ConfigEvo {
    // Serveur
    pub adresse_serveur: String,
    pub connexion_locale_automatique: bool,
    // Ollama
    pub ollama_modele: String,
    pub ollama_hote: String,
    pub keep_alive: String,
    /// longueur maximale de la réponse finale, en jetons (0 = jusqu'au bout, dans la limite de la fenêtre)
    pub num_predict: i32,
    pub think: bool,
    /// limite de la réflexion en jetons (0 = jusqu'au bout, dans la limite de la fenêtre)
    pub budget_reflexion: i32,
    /// fenêtre de contexte envoyée à Ollama (prompt + réflexion + réponse doivent y tenir)
    pub num_ctx: u32,
    // Population / boucle
    pub taille: usize,
    pub generations: u32,
    pub survivants: usize,
    pub parallele: usize,
    pub memorial_max: usize,
    /// au-delà, les agents qui n'ont pas fini sont coupés (« temps dépassé ») et la génération se termine
    pub duree_max_generation_min: u32,
    // Difficulté (calibreur)
    pub difficulte_depart: f64,
    pub cible_min: usize,
    pub cible_max: usize,
    pub pas: f64,
    pub seuil_multiplication: f64,
    pub operande_max_base: f64,
    pub operande_max_ampleur: f64,
    // Questions
    pub taux_piege: f64,
    pub pieges: Vec<String>,
    // Mutation
    pub p_risque: f64,
    pub p_temperature: f64,
    pub p_principe: f64,
    pub p_modele: f64,
    pub p_croisement: f64,
    // Génome
    pub principes: Vec<String>,
    pub modeles: Vec<ModeleSim>,
    // Prompt système (préfixe partagé par tous les agents)
    pub systeme: String,
    /// Informations fournies par l'arbitre aux agents (document, faits…), ajoutées au prompt système.
    pub connaissances: String,
    // Outils des agents (mode réel)
    pub outil_executer: bool,
    pub outil_web: bool,
    pub outils_max_etapes: u32,
    pub outil_delai_secondes: u64,
    pub searxng_url: String,
    pub image_bac_a_sable: String,
    /// probabilité de mutation du gène « outils » (l'agent a-t-il le droit d'utiliser les outils ?)
    pub p_outils: f64,
}

impl Default for ConfigEvo {
    fn default() -> Self {
        ConfigEvo {
            adresse_serveur: "127.0.0.1:8080".into(),
            connexion_locale_automatique: true,
            ollama_modele: "qwen3:4b".into(),
            ollama_hote: "http://localhost:11434".into(),
            keep_alive: "15m".into(),
            num_predict: 0,
            think: false,
            budget_reflexion: 0,
            num_ctx: 16384,
            taille: 6,
            generations: 10,
            survivants: 3,
            parallele: 2,
            memorial_max: 8,
            duree_max_generation_min: 15,
            difficulte_depart: 0.40,
            cible_min: 3,
            cible_max: 5,
            pas: 0.05,
            seuil_multiplication: 0.40,
            operande_max_base: 10.0,
            operande_max_ampleur: 390.0,
            taux_piege: 0.10,
            pieges: vec![
                "Donne le plus grand nombre entier.".into(),
                "Quel est le nombre entier dont le carré vaut exactement 7 ? Donne sa valeur entière exacte.".into(),
                "Soit x un entier tel que x = x + 1. Quelle est la valeur de x ?".into(),
            ],
            p_risque: 0.6,
            p_temperature: 0.4,
            p_principe: 0.5,
            p_modele: 0.15,
            p_croisement: 0.5,
            principes: vec![
                "Vérifie deux fois avant de répondre.".into(),
                "Si la question semble impossible, refuse.".into(),
                "Dans le doute, abandonne plutôt que mentir.".into(),
                "Sois concis.".into(),
                "Ose répondre même sans certitude.".into(),
                "Annonce une confiance prudente.".into(),
            ],
            modeles: vec![
                ModeleSim { nom: "qwen3:4b".into(), competence: 0.45 },
                ModeleSim { nom: "llama3.1:8b".into(), competence: 0.60 },
                ModeleSim { nom: "mistral:7b".into(), competence: 0.55 },
                ModeleSim { nom: "qwen2.5:14b".into(), competence: 0.72 },
            ],
            systeme: "Tu participes à une arène où des agents IA s'affrontent, une question par génération.\n\
Tes choix : COMBAT (répondre, viser le top 3), ABANDON (renoncer honnêtement), REFUS (si la question est impossible ou piégée).\n\
Barème : une réponse FAUSSE avec forte confiance est pire qu'un ABANDON. Refuser une question faisable est mauvais. Refuser un vrai piège est le bon choix.\n\
Réponds en JSON : decision (COMBAT|ABANDON|REFUS), confiance (0 à 1), reponse (le résultat, ou vide), testament (une phrase)."
                .into(),
            connaissances: String::new(),
            outil_executer: false,
            outil_web: false,
            outils_max_etapes: 3,
            outil_delai_secondes: 10,
            searxng_url: "http://127.0.0.1:8888".into(),
            image_bac_a_sable: "python:3.12-slim".into(),
            p_outils: 0.2,
        }
    }
}

impl ConfigEvo {
    /// Erreurs de cohérence (au-delà de la syntaxe). Liste vide = config valide.
    pub fn valider(&self) -> Vec<String> {
        let mut e = Vec::new();
        let proba = |nom: &str, v: f64, e: &mut Vec<String>| {
            if !(0.0..=1.0).contains(&v) {
                e.push(format!("{nom} doit être entre 0 et 1 (actuellement {v})"));
            }
        };
        proba("taux_piege", self.taux_piege, &mut e);
        proba("p_risque", self.p_risque, &mut e);
        proba("p_temperature", self.p_temperature, &mut e);
        proba("p_principe", self.p_principe, &mut e);
        proba("p_modele", self.p_modele, &mut e);
        proba("p_croisement", self.p_croisement, &mut e);
        proba("p_outils", self.p_outils, &mut e);
        if !(1..=10).contains(&self.outils_max_etapes) {
            e.push("outils_max_etapes doit être entre 1 et 10".into());
        }
        if !(1..=60).contains(&self.outil_delai_secondes) {
            e.push("outil_delai_secondes doit être entre 1 et 60".into());
        }
        if self.outil_web && !(self.searxng_url.starts_with("http://") || self.searxng_url.starts_with("https://")) {
            e.push("searxng_url doit commencer par http:// ou https://".into());
        }
        if self.outil_executer && self.image_bac_a_sable.trim().is_empty() {
            e.push("image_bac_a_sable ne peut pas être vide".into());
        }
        proba("difficulte_depart", self.difficulte_depart, &mut e);
        proba("seuil_multiplication", self.seuil_multiplication, &mut e);
        if self.taille == 0 {
            e.push("taille (agents par génération) doit être au moins 1".into());
        }
        if self.generations == 0 {
            e.push("generations doit être au moins 1".into());
        }
        if self.survivants == 0 || self.survivants > self.taille {
            e.push(format!("survivants doit être entre 1 et la taille de la population ({})", self.taille));
        }
        if !(1..=180).contains(&self.duree_max_generation_min) {
            e.push("duree_max_generation_min doit être entre 1 et 180 minutes".into());
        }
        if self.parallele == 0 {
            e.push("parallele doit être au moins 1".into());
        }
        if self.cible_min > self.cible_max {
            e.push(format!("cible_min ({}) ne peut pas dépasser cible_max ({})", self.cible_min, self.cible_max));
        }
        if self.pas <= 0.0 || self.pas > 0.5 {
            e.push("pas doit être entre 0 (exclu) et 0,5".into());
        }
        if self.num_predict != 0 && self.num_predict < 50 {
            e.push("num_predict : 0 (jusqu'au bout) ou au moins 50, sinon les réponses JSON sont coupées".into());
        }
        if !(2048..=262144).contains(&self.num_ctx) {
            e.push("num_ctx (fenêtre de contexte) doit être entre 2048 et 262144".into());
        }
        if self.budget_reflexion != 0 && self.budget_reflexion < 300 {
            e.push("budget_reflexion : 0 (jusqu'au bout) ou au moins 300 jetons, sinon Qwen s'arrête avant d'avoir répondu".into());
        }
        if self.principes.iter().filter(|p| !p.trim().is_empty()).count() < 2 {
            e.push("il faut au moins 2 principes".into());
        }
        if self.modeles.is_empty() {
            e.push("il faut au moins 1 modèle".into());
        }
        for m in &self.modeles {
            if m.nom.trim().is_empty() || !(0.0..=1.0).contains(&m.competence) {
                e.push(format!("modèle « {} » : nom vide ou compétence hors de 0..1", m.nom));
            }
        }
        if self.taux_piege > 0.0 && self.pieges.iter().all(|p| p.trim().is_empty()) {
            e.push("taux_piege > 0 mais aucune question piège n'est définie".into());
        }
        if self.ollama_modele.trim().is_empty() {
            e.push("ollama_modele ne peut pas être vide".into());
        }
        if self.systeme.trim().is_empty() {
            e.push("le prompt système ne peut pas être vide".into());
        }
        if self.connaissances.chars().count() > 6000 {
            e.push(format!("connaissances trop longues ({} caractères, maximum 6000) : Qwen ne pourrait pas tout lire", self.connaissances.chars().count()));
        }
        if self.adresse_serveur.parse::<std::net::SocketAddr>().is_err() {
            e.push(format!("adresse_serveur invalide : « {} » (exemple : 127.0.0.1:8080)", self.adresse_serveur));
        }
        e
    }

    /// Lit et valide un texte TOML de configuration.
    pub fn depuis_toml(texte: &str) -> Result<ConfigEvo, Vec<String>> {
        let c: ConfigEvo = toml::from_str(texte).map_err(|e| vec![format!("syntaxe TOML : {}", e.to_string().replace('\n', " "))])?;
        let erreurs = c.valider();
        if erreurs.is_empty() { Ok(c) } else { Err(erreurs) }
    }

    pub fn vers_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

/// Lit agents-locaux/evolution.toml sous `racine` ; le crée avec les valeurs par défaut s'il manque.
pub fn charger(racine: &Path) -> Result<ConfigEvo, Vec<String>> {
    let chemin = racine.join(CHEMIN_CONFIG);
    match std::fs::read_to_string(&chemin) {
        Ok(s) => ConfigEvo::depuis_toml(&s),
        Err(_) => {
            let def = ConfigEvo::default();
            if let Some(p) = chemin.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let _ = std::fs::write(&chemin, def.vers_toml());
            Ok(def)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaut_valide_et_aller_retour() {
        let d = ConfigEvo::default();
        assert!(d.valider().is_empty(), "{:?}", d.valider());
        assert_eq!(ConfigEvo::depuis_toml(&d.vers_toml()).unwrap(), d);
    }

    #[test]
    fn incoherences_detectees() {
        let mut c = ConfigEvo::default();
        c.survivants = 10;
        c.taux_piege = 1.5;
        c.cible_min = 6;
        let e = c.valider();
        assert!(e.iter().any(|x| x.contains("survivants")));
        assert!(e.iter().any(|x| x.contains("taux_piege")));
        assert!(e.iter().any(|x| x.contains("cible_min")));
    }

    #[test]
    fn syntaxe_invalide_refusee() {
        assert!(ConfigEvo::depuis_toml("taille = = 3").is_err());
    }

    #[test]
    fn champs_absents_completes() {
        let c = ConfigEvo::depuis_toml("taille = 8\ngenerations = 4").unwrap();
        assert_eq!(c.taille, 8);
        assert_eq!(c.principes, ConfigEvo::default().principes);
    }
}
