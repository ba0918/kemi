// 固定の翻訳文だけを扱い、利用者の入力をHTMLとして挿入しない。
const messages = {
  "en": {
    "copy0": "kemi — Read the change. Share your intent.",
    "copy1": "kemi is a local review tool. Read diffs in your browser and return line comments and suggestions as JSON.",
    "copy2": "Skip to content",
    "copy3": "kemi home",
    "copy4": "Main navigation",
    "copy5": "How it works",
    "copy6": "Read the change.<br><span>Share your intent.</span>",
    "copy7": "After the code comes the review.<br>Read the diff. Leave a thought on the line.<br>Send your intent back to the agent.",
    "copy8": "kemi is a review tool that runs locally.",
    "copy9": "Get started with kemi <span aria-hidden=\"true\">↗</span>",
    "copy10": "Single binary <i>/</i> Browser review <i>/</i> JSON output",
    "copy11": "The actual kemi UI: a change from input to input.trim() in src/greeting.ts, with a comment suggesting Guest for an empty name.",
    "copy12": "assets/review-en-desktop.png",
    "copy13": "assets/review-en-mobile.png",
    "copy14": "01 — Actual kemi UI / Synthetic sample data",
    "copy15": "A screenshot of a review, not an interactive session.",
    "copy16": "A human reads.<br>An agent receives.",
    "copy17": "A place to think and communicate,<br>between terminal and browser.",
    "copy18": "Open the diff",
    "copy19": "Working tree, staged changes, or a commit range. Pass the changes you want to review to the CLI, and your browser opens.",
    "copy20": "Put intent on the line",
    "copy21": "Comment where it matters. Add replacement code as a suggestion to show exactly what you want to change.",
    "copy22": "Line comments + suggestions",
    "copy23": "Return your review",
    "copy24": "Approve or request changes. Your comments and suggestions return to the terminal that started kemi as one JSON document.",
    "copy25": "Standard output → agent",
    "copy26": "<b>{ }</b> See the JSON an agent receives <small>Sample excerpt</small>",
    "copy27": "Line numbers, comments, and suggestions keep their structure as they return to the agent. The agent applies the suggested changes.",
    "copy28": "Read the complete JSON contract ↗",
    "copy29": "Give the change<br>your attention.",
    "copy30": "The views you need, close at hand.<br>The judgment stays with you.",
    "copy31": "See the result. Follow the history.",
    "copy32": "Switch a commit range between its final form and a per-commit view. Follow a changed block back to the commits that made it.",
    "copy33": "Look beyond the source.",
    "copy34": "Markdown, CSV / TSV, and images also have rendered views. Check the source diff alongside the visual change.",
    "copy35": "Read locally. Return suggestions.",
    "copy36": "kemi listens on loopback by default. It never edits your files; suggested changes are returned as JSON.",
    "copy37": "Try kemi on<br>your next change.",
    "copy38": "The name comes from the Japanese “kemi suru”: to review.<br>Start by opening a diff in your own repository.",
    "copy39": "Read the usage guide <span aria-hidden=\"true\">↗</span>",
    "copy40": "1. Install with mise",
    "copy41": "2. Run inside a Git repository",
    "copy42": "Install from a ZIP on Windows",
    "copy43": "Download <code>kemi-&lt;target&gt;.zip</code> for your CPU from the <a href=\"https://github.com/ba0918/kemi/releases/latest\">latest GitHub Release</a>. Extract <code>kemi.exe</code> and add it to your PATH. Git must also be on your PATH.",
    "copy44": "Give the change a closer look.",
    "copy45": "{\n  \"kemi\": 1,\n  \"title\": \"Normalize input\",\n  \"verdict\": \"changes_requested\",\n  \"approval\": [],\n  \"comments\": [\n    {\n      \"path\": \"src/greeting.ts\",\n      \"side\": \"new\",\n      \"start_line\": 2,\n      \"end_line\": 2,\n      \"body\": \"Please handle whitespace-only input too.\\nCould an empty name fall back to Guest?\",\n      \"suggestion\": {\n        \"replacement\": \"  const name = input.trim() || \\\"Guest\\\";\"\n      }\n    }\n  ]\n}",
    "language": "Language"
  },
  "ja": {
    "copy0": "kemi — 変更を読み、意図を返す。",
    "copy1": "kemiは、差分をブラウザで読み、行コメントと修正提案をJSONで返すローカルなレビュー道具です。",
    "copy2": "本文へ移動",
    "copy3": "kemi トップ",
    "copy4": "メインナビゲーション",
    "copy5": "使い方",
    "copy6": "変更を読み、<br><span>意図を返す。</span>",
    "copy7": "コードをつくる、その次に。<br>差分をブラウザで読み、気づきを行に残す。<br>あなたのレビューを、エージェントへ。",
    "copy8": "kemi は、ローカルで動くレビュー道具です。",
    "copy9": "kemi をはじめる <span aria-hidden=\"true\">↗</span>",
    "copy10": "単一バイナリ <i>／</i> ブラウザでレビュー <i>／</i> JSONで受け渡し",
    "copy11": "kemiの実画面。src/greeting.tsのinputをinput.trim()に変える差分に、空文字ならGuestを使う提案コメントが付いています。",
    "copy12": "assets/review-desktop.png",
    "copy13": "assets/review-mobile.png",
    "copy14": "01 — 実際のkemiの画面 / 架空のサンプルデータ",
    "copy15": "この画面は操作用ではなく、表示例です。",
    "copy16": "読むのは、人。<br>受け取るのは、<br class=\"mobile-break\">エージェント。",
    "copy17": "ターミナルとブラウザのあいだに、<br>考えて、伝えるための場所を。",
    "copy18": "差分をひらく",
    "copy19": "作業ツリー、ステージ済みの変更、コミット範囲。読みたい変更をCLIから渡すと、ブラウザが開きます。",
    "copy20": "行に、意図を残す",
    "copy21": "気になる行にコメント。置き換えたいコードは修正提案として添えて、どこをどう変えたいか伝えます。",
    "copy22": "行コメント ＋ 修正提案",
    "copy23": "レビューを返す",
    "copy24": "承認か変更要求でレビューを終えると、コメントと提案がひとつのJSONになり、起動元のターミナルへ返ります。",
    "copy25": "標準出力 → エージェント",
    "copy26": "<b>{ }</b> エージェントに返るJSONを見る <small>サンプル・抜粋</small>",
    "copy27": "画面上の行番号、コメント、修正提案を、構造を保ったまま受け渡します。提案を適用するのはエージェントです。",
    "copy28": "完全なJSON仕様を読む ↗",
    "copy29": "変更に、<br>目を向けるために。",
    "copy30": "必要な見方を、手元に。<br>レビューの判断は、あなたに。",
    "copy31": "全体と経緯を、行き来する。",
    "copy32": "コミット範囲は「最終形」と「コミットごと」で切り替え。変更ブロックから、その由来もたどれます。",
    "copy33": "コード以外も、見て確かめる。",
    "copy34": "Markdown、CSV / TSV、画像には描画表示も。ソースの差分と、見た目の変化を確認できます。",
    "copy35": "手元で読み、提案として返す。",
    "copy36": "既定の接続先はループバック。kemiはファイルを書き換えず、修正提案をJSONとして返します。",
    "copy37": "次の変更で、<br>けみしてみる。",
    "copy38": "名前の由来は「閲する（けみする）」。<br>まずは、手元の差分をひらくところから。",
    "copy39": "使い方を読む <span aria-hidden=\"true\">↗</span>",
    "copy40": "1. mise でインストール",
    "copy41": "2. Gitリポジトリ内で実行",
    "copy42": "WindowsでZIPから導入する",
    "copy43": "<a href=\"https://github.com/ba0918/kemi/releases/latest\">最新のGitHub Release</a>から、お使いのCPUに合う <code>kemi-&lt;target&gt;.zip</code> をダウンロード。展開した <code>kemi.exe</code> をPATHに追加してください。GitもPATHに必要です。",
    "copy44": "変更と、向き合う。",
    "copy45": "{\n  \"kemi\": 1,\n  \"title\": \"入力の正規化\",\n  \"verdict\": \"changes_requested\",\n  \"approval\": [],\n  \"comments\": [{\n    \"path\": \"src/greeting.ts\",\n    \"side\": \"new\",\n    \"start_line\": 2,\n    \"end_line\": 2,\n    \"body\": \"空白だけの入力も考慮したいです。\\n名前が空なら、ゲストとして扱いませんか？\",\n    \"suggestion\": {\n      \"replacement\": \"  const name = input.trim() || \\\"Guest\\\";\"\n    }\n  }]\n}",
    "language": "言語"
  }
};

const languageKey = 'kemi-site-language';
function showLanguage(language) {
  const copy = messages[language];
  document.documentElement.lang = language;
  document.querySelectorAll('[data-i18n]').forEach(element => {
    element.innerHTML = copy[element.dataset.i18n];
  });
  for (const attribute of ['content', 'aria-label', 'alt', 'src', 'srcset']) {
    document.querySelectorAll('[data-i18n-' + attribute + ']').forEach(element => {
      element.setAttribute(attribute, copy[element.getAttribute('data-i18n-' + attribute)]);
    });
  }
  document.querySelectorAll('[data-language]').forEach(button => {
    button.setAttribute('aria-pressed', String(button.dataset.language === language));
  });
}
let initialLanguage = 'en';
try {
  const saved = localStorage.getItem(languageKey);
  if (saved === 'ja' || saved === 'en') initialLanguage = saved;
} catch { /* 保存を許可しないブラウザでも言語切替は使える。 */ }
showLanguage(initialLanguage);
document.querySelectorAll('[data-language]').forEach(button => {
  button.addEventListener('click', () => {
    const language = button.dataset.language;
    showLanguage(language);
    try { localStorage.setItem(languageKey, language); }
    catch { /* 保存できない場合も現在の選択は適用済み。 */ }
  });
});
