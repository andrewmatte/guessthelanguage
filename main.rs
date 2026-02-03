/*[dependencies]
actix = "0.13.5"
actix-web = "4.12.1"
lazy_static = "1.5.0"
rand = "0.9.2"
serde = { version = "1.0.228", features = ["derive"] }
walkdir = "2.5.0"*/


use actix_web::{App, HttpResponse, HttpServer, Responder, get, web};
use lazy_static::lazy_static;
use rand::prelude::IndexedRandom;
use rand::prelude::IteratorRandom;
use rand::rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

/* ---------------- Language Aliases ---------------- */

lazy_static! {
    static ref LANG_MAP: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("af", "Afrikaans");
        m.insert("pa", "Punjabi");
        m.insert("an", "Aragonese");
        m.insert("br", "Breton");
        m.insert("fa-ir", "Persian");
        m.insert("lo", "Laotian");
        m.insert("eo", "esperanto");
        m.insert("ca", "Catalan");
        m.insert("ca-valencia", "Valencian");
        m.insert("en ", "English");
        m.insert("en", "South African");
        m.insert("gug", "Guarani");
        m.insert("is", "Icelandic");
        m.insert("fa", "Persian");
        m.insert("mn", "Mongolian");
        m.insert("ku", "Kurdish");
        m.insert("lt", "Lithuanian");
        m.insert("lv", "Latvian");
        m.insert("md", "Mapudüngun");
        m.insert("mr", "Marathi");
        m.insert("tr", "Turkish");
        m.insert("as", "Assamese");
        m.insert("sq", "Albanian");
        m.insert("bo", "Tibetan");
        m.insert("nl", "Dutch");
        m.insert("ne", "Nepali");
        m.insert("kn", "Kannada");
        m.insert("gu", "Bengali");
        m.insert("bn", "Nepali");
        m.insert("ne", "Nepali");
        m.insert("da", "Danish");
        m.insert("hr", "Croatian");
        m.insert("hi", "Hindi");
        m.insert("bg", "Bulgarian");
        m.insert("no", "Norwegian");
        m.insert("nn", "Norwegian");
        m.insert("nb", "Norwegian");
        m.insert("si", "Sinhala");
        m.insert("ru", "Russian");
        m.insert("oc", "Occitan");
        m.insert("es", "Spanish");
        m.insert("in", "Indonesian");
        m.insert("en", "English");
        m.insert("it", "Italian");
        m.insert("fr", "French");
        m.insert("gd", "Scottish Gaelic");
        m.insert("hu", "Hungarian");
        m.insert("sw", "Swahili");
        m.insert("be", "Belarusian");
        m.insert("be-official", "Belarusian");
        m.insert("pl", "Polish");
        m.insert("sk", "Slovak");
        m.insert("ar", "Arabic");
        m.insert("sa", "Sanskrit");
        m.insert("de", "German");
        m.insert("pt", "Portuguese");
        m.insert("ro", "Romanian");
        m.insert("pt", "Portuguese");
        m.insert("bs", "Bosnian");
        m.insert("gl", "Galician");
        m.insert("he", "Hebrew");
        m.insert("cs", "Czech");
        m.insert("el", "Greek");
        m.insert("id", "Indonesian");
        m.insert("ko", "Korean");
        m.insert("et", "Estonian");
        m.insert("or", "Oriya");
        m.insert("sl", "Slovenian");
        m.insert("uk", "Ukrainian");
        m.insert("ta", "Tamil");
        m.insert("kmr", "Kurdish");
        m.insert("sv", "Swedish");
        m.insert("th", "Thai");
        m.insert("fr", "French");
        m.insert("hyph", "Oops");
        m.insert("vi", "Vietnamese");
        m.insert("sr", "Serbian");
        m.insert("sr-latn", "Serbian");
        m.insert("ckb", "Kurdish");
        m.insert("te", "Telugu");
        m
    };
}

/* ---------------- Constants ---------------- */

const WORDS_PER_ROUND: usize = 10;
const MIN_WORD_LEN: usize = 3;

/* ---------------- Data Models ---------------- */

#[derive(Serialize)]
struct GamePayload {
    words: Vec<String>,
    answer: String,
    valid_answers: Vec<String>,
    language_code: String,
}

#[derive(Deserialize)]
struct HintQuery {
    language: String,
}

/* ---------------- RAM-resident Language ---------------- */

#[derive(Clone)]
struct LanguageData {
    code: String,
    base: String,
    name: String,
    words: Vec<String>,
    valid_answers: Vec<String>,
}

/* ---------------- Paths ---------------- */

fn base_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap()).join(".langgame")
}

fn repo_dir() -> PathBuf {
    base_dir().join("dictionaries")
}

/* ---------------- Repo Setup ---------------- */

fn ensure_repo() {
    fs::create_dir_all(base_dir()).unwrap();

    if !repo_dir().exists() {
        Command::new("git")
            .args([
                "clone",
                "--depth=1",
                "https://github.com/LibreOffice/dictionaries.git",
                repo_dir().to_str().unwrap(),
            ])
            .status()
            .expect("git clone failed");

        fs::remove_dir_all(repo_dir().join(".git")).ok();
        fs::remove_dir_all(repo_dir().join(".github")).ok();
        fs::remove_dir_all(repo_dir().join("util")).ok();
    }
}

/* ---------------- Dictionary Discovery ---------------- */

struct RawBundle {
    code: String,
    base: String,
    dic: PathBuf,
}

fn discover_dictionaries() -> Vec<RawBundle> {
    let mut out = Vec::new();

    for entry in WalkDir::new(repo_dir())
        .min_depth(1)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
    {
        let dir_name = match entry.file_name().to_str() {
            Some(n) => n.to_lowercase(),
            None => continue,
        };

        let base = dir_name.split('_').next().unwrap_or(&dir_name).to_string();

        let dic = fs::read_dir(entry.path()).ok().and_then(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .find(|p| p.extension().map(|e| e == "dic").unwrap_or(false))
        });

        if let Some(dic) = dic {
            out.push(RawBundle {
                code: base.clone(),
                base,
                dic,
            });
        }
    }

    out
}

/* ---------------- Load into RAM ---------------- */

fn load_dictionary(dic: &Path) -> Vec<String> {
    BufReader::new(File::open(dic).unwrap())
        .lines()
        .filter_map(Result::ok)
        .map(|l| l.split('/').next().unwrap_or("").to_string())
        .filter(|w| w.len() >= MIN_WORD_LEN && w.chars().all(|c| c.is_alphabetic()))
        .collect()
}

fn build_valid_answers(code: &str, name: &str) -> Vec<String> {
    let mut set = HashSet::new();
    let lc = code.to_lowercase();
    let base = lc.split('_').next().unwrap_or(&lc);

    set.insert(lc.clone());
    set.insert(base.to_string());
    set.insert(name.to_lowercase());

    set.into_iter().collect()
}

fn load_all_languages() -> Vec<LanguageData> {
    discover_dictionaries()
        .into_iter()
        .filter_map(|b| {
            let name = LANG_MAP
                .get(b.code.as_str())
                .or_else(|| LANG_MAP.get(b.base.as_str()))?
                .to_string();

            let words = load_dictionary(&b.dic);
            if words.len() < WORDS_PER_ROUND {
                return None;
            }

            Some(LanguageData {
                code: b.code,
                base: b.base,
                name: name.clone(),
                valid_answers: build_valid_answers(&name, &name),
                words,
            })
        })
        .collect()
}

/* ---------------- Game Logic ---------------- */

fn sample(words: &[String]) -> Vec<String> {
    let mut rng = rand::rng();
    words
        .choose_multiple(&mut rng, WORDS_PER_ROUND)
        .cloned()
        .collect()
}

/* ---------------- HTTP ---------------- */

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(INDEX_HTML)
}

#[get("/game/new")]
async fn new_game_http(languages: web::Data<Vec<LanguageData>>) -> impl Responder {
    let lang = languages.iter().choose(&mut rng()).expect("no languages");

    HttpResponse::Ok().json(GamePayload {
        words: sample(&lang.words),
        answer: lang.name.clone(),
        valid_answers: lang.valid_answers.clone(),
        language_code: lang.code.clone(),
    })
}

#[get("/game/hint")]
async fn hint(languages: web::Data<Vec<LanguageData>>, q: web::Query<HintQuery>) -> impl Responder {
    let req = q.language.to_lowercase();

    let lang = match languages.iter().find(|l| l.code == req || l.base == req) {
        Some(l) => l,
        None => return HttpResponse::BadRequest().body("Unknown language"),
    };

    HttpResponse::Ok().json(sample(&lang.words))
}

/* ---------------- Main ---------------- */

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    ensure_repo();

    println!("Loading dictionaries into RAM...");
    let languages = load_all_languages();
    println!("Loaded {} languages", languages.len());

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(languages.clone()))
            .service(index)
            .service(new_game_http)
            .service(hint)
    })
    .bind(("0.0.0.0", 8000))?
    .run()
    .await
}

/* ---------------- HTML ---------------- */

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">

<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">

  <title>Language Guess</title>

  <style>
    .autocomplete {
      position: relative;
      flex: 1 1 100%;
    }

    .autocomplete-list {
      position: absolute;
      top: 100%;
      left: 0;
      right: 0;
      background: #fff;
      border: 1px solid #ccc;
      border-top: none;
      max-height: 160px;
      overflow-y: auto;
      z-index: 10;
    }

    .autocomplete-item {
      padding: 8px 10px;
      cursor: pointer;
    }

    .autocomplete-item:hover,
    .autocomplete-item.active {
      background: #eee;
    }


    :root {
      --max-width: 720px;
    }

    * {
      box-sizing: border-box;
    }

    body {
      font-family: system-ui, -apple-system, BlinkMacSystemFont, sans-serif;
      margin: 0;
      padding: 16px;
      background: #fff;
    }

    .gamebox {
      max-width: var(--max-width);
      margin: 0 auto;
      padding: 20px;
    }

    h1 {
      margin-top: 0;
      font-size: 1.6rem;
    }

    #words {
      margin: 16px 0;
      line-height: 1.5;
      word-break: break-word;
      white-space: pre-wrap;
    }

    .controls {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      margin-bottom: 12px;
    }

    input#guess {
      flex: 1 1 100%;
      padding: 10px;
      font-size: 1rem;
    }

    button {
      flex: 1 1 100%;
      padding: 10px;
      font-size: 1rem;
      cursor: pointer;
    }

    pre#out {
      margin-top: 16px;
      white-space: pre-wrap;
      font-size: 1rem;
    }

    /* Larger screens */
    @media (min-width: 600px) {
      input#guess {
        flex: 2 1 auto;
      }

      button {
        flex: 0 0 auto;
      }
    }
  </style>
</head>

<body>
  <div class="gamebox">
    <h1>Guess the Language</h1>
    <p>Guess the language from these ten words. Press hint to get 10 more words from the same language.</p>

    <div id="words"></div>

    <div class="controls">
      <div class="autocomplete">
        <input id="guess" placeholder="Enter language name" autocomplete="off">
        <div id="autocomplete-list" class="autocomplete-list" hidden></div>
      </div>

      <button onclick="checkGuess()">Guess</button>
      <button onclick="hint()">Hint</button>
      <button onclick="newGame()" id="new_game">New</button>
    </div>

    <pre id="out"></pre>

    <h2>Your Scores</h2>
    <pre id="scores"></pre>

  </div>



  <script>

    let currentGame = null;

    async function newGame() {
      const r = await fetch('/game/new');
      currentGame = await r.json();

      document.getElementById('words').innerText =
        currentGame.words.join(', ');

      document.getElementById('out').innerText = '';
      document.getElementById('guess').value = '';
      document.getElementById('guess').focus();
    }

    async function hint() {
      if (!currentGame) return;

      const r = await fetch(
        '/game/hint?language=' + encodeURIComponent(currentGame.language_code)
      );

      const words = await r.json();
      document.getElementById('words').innerText +=
        '\n' + words.join(', ');
    }

function checkGuess() {
  if (!currentGame) return;

  const g = document.getElementById('guess').value
    .trim()
    .toLowerCase();

  const correct = currentGame.valid_answers.includes(g);
  const language = currentGame.answer;

  document.getElementById('out').innerText =
    correct
      ? 'Correct!'
      : 'Incorrect: ' + language;

  updateScore(language, correct);

  document.getElementById('new_game').focus();
}


    newGame();
  </script>

  <script>
    const LANGUAGES = ["Afrikaans",
      "Albanian",
      "Arabic",
      "Aragonese",
      "Assamese",
      "Belarusian",
      "Bengali",
      "Bosnian",
      "Breton",
      "Bulgarian",
      "Catalan",
      "Croatian",
      "Czech",
      "Danish",
      "Dutch",
      "English",
      "esperanto",
      "Estonian",
      "French",
      "Galician",
      "German",
      "Greek",
      "Guarani",
      "Hebrew",
      "Hindi",
      "Hungarian",
      "Icelandic",
      "Indonesian",
      "Italian",
      "Kannada",
      "Korean",
      "Kurdish",
      "Laotian",
      "Latvian",
      "Lithuanian",
      "Mapudüngun",
      "Marathi",
      "Mongolian",
      "Nepali",
      "Norwegian",
      "Occitan",
      "Oops",
      "Oriya",
      "Persian",
      "Polish",
      "Portuguese",
      "Punjabi",
      "Romanian",
      "Russian",
      "Scottish Gaelic",
      "Serbian",
      "Sinhala",
      "Slovak",
      "Slovenian",
      "Spanish",
      "Swahili",
      "Swedish",
      "Tamil",
      "Telugu",
      "Thai",
      "Tibetan",
      "Turkish",
      "Ukrainian",
      "Valencian",
      "Vietnamese"];
    const input = document.getElementById("guess");
    const list = document.getElementById("autocomplete-list");

    let activeIndex = -1;

    function closeList() {
      list.hidden = true;
      list.innerHTML = "";
      activeIndex = -1;
    }

    function renderList(matches) {
      list.innerHTML = "";
      activeIndex = -1;

      if (matches.length === 0) {
        closeList();
        return;
      }

      matches.forEach((lang, i) => {
        const div = document.createElement("div");
        div.className = "autocomplete-item";
        div.textContent = lang;
        div.onclick = () => {
          input.value = lang;
          closeList();
        };
        list.appendChild(div);
      });

      list.hidden = false;
    }

    input.addEventListener("input", () => {
      const value = input.value.trim().toLowerCase();

      if (!value) {
        closeList();
        return;
      }

      const matches = LANGUAGES.filter(l =>
        l.toLowerCase().startsWith(value)
      );

      renderList(matches);
    });

    input.addEventListener("keydown", e => {
      const items = list.querySelectorAll(".autocomplete-item");
      if (list.hidden || items.length === 0) return;

      if (e.key === "ArrowDown") {
        e.preventDefault();
        activeIndex = (activeIndex + 1) % items.length;
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        activeIndex = (activeIndex - 1 + items.length) % items.length;
      } else if (e.key === "Enter") {
        if (activeIndex >= 0) {
          e.preventDefault();
          input.value = items[activeIndex].textContent;
          closeList();
        }
        return;
      } else if (e.key === "Escape") {
        closeList();
        return;
      }

      items.forEach((el, i) =>
        el.classList.toggle("active", i === activeIndex)
      );
    });

    document.addEventListener("click", e => {
      if (!e.target.closest(".autocomplete")) {
        closeList();
      }
    });
    const SCORE_KEY = "language_scores";

    function loadScores() {
      return JSON.parse(localStorage.getItem(SCORE_KEY) || "{}");
    }

    function saveScores(scores) {
      localStorage.setItem(SCORE_KEY, JSON.stringify(scores));
    }

    function updateScore(language, correct) {
      const scores = loadScores();

      if (!scores[language]) {
        scores[language] = { correct: 0, attempts: 0 };
      }

      scores[language].attempts += 1;
      if (correct) scores[language].correct += 1;

      saveScores(scores);
      renderScores();
    }

    function renderScores() {
      const scores = loadScores();

      const sorted = Object.entries(scores)
        .map(([lang, s]) => ({
          lang,
          correct: s.correct,
          attempts: s.attempts,
          accuracy: s.correct / s.attempts
        }))
        .sort((a, b) => b.accuracy - a.accuracy);

      const lines = sorted.map(s =>
        `${s.lang.padEnd(20)} ${s.correct}/${s.attempts} (${(s.accuracy * 100).toFixed(1)}%)`
      );

      document.getElementById("scores").innerText =
        lines.length ? lines.join("\n") : "No guesses yet.";
    }
renderScores();
  </script>


</body>

</html>
"#;
