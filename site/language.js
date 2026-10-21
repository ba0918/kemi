// 固定の翻訳文だけを扱い、利用者の入力をHTMLとして挿入しない。
const messages = {
  "en": {
    "copy0": "kemi — Read the change. Share your intent.",
    "copy1": "kemi is a local review tool. Read explanations and changes, ask an AI agent questions, and return your decision and comments as JSON.",
    "copy2": "Skip to content",
    "copy3": "kemi home",
    "copy4": "Main navigation",
    "copy5": "How it works",
    "copy6": "Read the change.<br><span>Share your intent.</span>",
    "copy7": "Read why the change was made.<br>Ask the agent while the review stays open.<br>Decide when you are ready.",
    "copy8": "kemi is a review tool that runs locally.",
    "copy9": "Get started with kemi <span aria-hidden=\"true\">↗</span>",
    "copy10": "Single binary <i>/</i> Browser review <i>/</i> JSON output",
    "copy11": "Actual kemi v0.3.2: a question and agent reply about using Guest for an empty name in src/greeting.ts. Desktop shows the diff and Conversation; mobile shows the conversation sheet.",
    "copy12": "assets/review-en-desktop.png",
    "copy13": "assets/review-en-mobile.png",
    "copy14": "01 — Actual kemi v0.3.2 / Synthetic sample data",
    "copy15": "A screenshot of a review, not an interactive session.",
    "copy16": "Read. Ask.<br>Then decide.",
    "copy17": "A place to think and communicate,<br>between terminal and browser.",
    "copy18": "Read the explanation",
    "copy19": "With an agent, start with an Explanation: what changed, why, and what needs your judgment. Follow its links to the code. You can also open a diff directly.",
    "copy20": "Ask before deciding",
    "copy21": "Comment on the explanation or a line. Press Hand to agent to send your feedback after the agent connects with kemi wait. Replies appear in Conversation; the review stays open.",
    "copy22": "Explanation + Conversation",
    "copy23": "Return your decision",
    "copy24": "Press Approve or Request changes when you are ready. Your decision, comments, suggestions, and conversation return to the starting terminal as one JSON document.",
    "copy25": "Standard output → agent",
    "copy26": "<b>{ }</b> See the JSON an agent receives <small>Sample excerpt</small>",
    "copy27": "This version-4 excerpt shows a file comment, a suggestion, and an agent reply. The complete result also records explanation comments and notes on the review. kemi never applies suggestions.",
    "copy28": "Read the complete JSON contract ↗",
    "copy29": "Give the change<br>your attention.",
    "copy30": "The views you need, close at hand.<br>The judgment stays with you.",
    "copy31": "Understand the reason. Inspect the code.",
    "copy32": "Read and comment on an Explanation before the diff. Follow its Code links, then switch a commit range between final form and per-commit views to inspect the history.",
    "copy33": "Read documents. Review a running page.",
    "copy34": "Markdown, CSV / TSV, and images have rendered views. With kemi --live, inspect a local running page at different widths, compare snapshots, and comment with Element, Arrow, or Pen.",
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
    "copy45": "{\n  \"kemi\": 4,\n  \"title\": \"Normalize input\",\n  \"verdict\": \"changes_requested\",\n  \"approval\": [],\n  \"comments\": [\n    {\n      \"target_type\": \"file\",\n      \"path\": \"src/greeting.ts\",\n      \"side\": \"new\",\n      \"start_line\": 2,\n      \"end_line\": 2,\n      \"body\": \"Could an empty name fall back to Guest?\",\n      \"suggestion\": {\n        \"replacement\": \"  const name = input.trim() || \\\"Guest\\\";\"\n      },\n      \"replies\": [\n        {\n          \"author\": \"agent\",\n          \"body\": \"trim() removes surrounding spaces. Please decide whether an empty name should use Guest.\",\n          \"parts\": null\n        }\n      ]\n    }\n  ]\n}",
    "language": "Language",
    "copy46": "Japanese usage guide ↗",
    "copy47": "Agent skill ↗"
  },
  "ja": {
    "copy0": "kemi — 変更を読み、意図を返す。",
    "copy1": "kemiは、説明と変更を読み、AIエージェントに質問し、判断とコメントをJSONで返すローカルなレビュー道具です。",
    "copy2": "本文へ移動",
    "copy3": "kemi トップ",
    "copy4": "メインナビゲーション",
    "copy5": "使い方",
    "copy6": "変更を読み、<br><span>意図を返す。</span>",
    "copy7": "何を、なぜ変えたのか読む。<br>レビューを開いたまま、エージェントに質問。<br>納得してから、判断を返す。",
    "copy8": "kemi は、ローカルで動くレビュー道具です。",
    "copy9": "kemi をはじめる <span aria-hidden=\"true\">↗</span>",
    "copy10": "単一バイナリ <i>／</i> ブラウザでレビュー <i>／</i> JSONで受け渡し",
    "copy11": "kemi v0.3.2の実画面。src/greeting.tsの空の名前をGuestにするか、質問とエージェントの返答を表示。デスクトップは差分と会話、モバイルは会話シートです。",
    "copy12": "assets/review-desktop.png",
    "copy13": "assets/review-mobile.png",
    "copy14": "01 — 実際のkemi v0.3.2 / 架空のサンプルデータ",
    "copy15": "この画面は操作用ではなく、表示例です。",
    "copy16": "説明を読む。質問する。<br>それから、判断する。",
    "copy17": "ターミナルとブラウザのあいだに、<br>考えて、伝えるための場所を。",
    "copy18": "まず、説明を読む",
    "copy19": "エージェントとのレビューは、変更内容・理由・判断してほしい点をまとめたExplanationから。リンクでコードへ進めます。差分を直接開くこともできます。",
    "copy20": "判断する前に、質問する",
    "copy21": "説明や行にコメント。エージェントがkemi waitで接続したら、Hand to agentでまとめて渡せます。返答はConversationに届き、レビューは開いたままです。",
    "copy22": "説明 ＋ レビュー中の対話",
    "copy23": "判断を返す",
    "copy24": "準備ができたらApprove（承認）かRequest changes（変更要求）。判断・コメント・修正提案・会話がひとつのJSONになり、起動元のターミナルへ返ります。",
    "copy25": "標準出力 → エージェント",
    "copy26": "<b>{ }</b> エージェントに返るJSONを見る <small>サンプル・抜粋</small>",
    "copy27": "形式4の抜粋です。行コメント・修正提案・エージェントの返答を示しています。完全な結果には説明へのコメントやレビュー全体への発言も含まれます。kemiは提案を適用しません。",
    "copy28": "完全なJSON仕様を読む ↗",
    "copy29": "変更に、<br>目を向けるために。",
    "copy30": "必要な見方を、手元に。<br>レビューの判断は、あなたに。",
    "copy31": "理由を読み、コードを確かめる。",
    "copy32": "差分の前にExplanationを読み、説明にもコメント。Codeリンクで該当箇所へ進み、コミット範囲の「最終形」と「コミットごと」で経緯をたどれます。",
    "copy33": "文書も、動くページも確かめる。",
    "copy34": "Markdown、CSV / TSV、画像は描画表示も。kemi --liveなら、手元で動くページの幅を変え、スナップショットと比べ、Element・Arrow・Penで場所にコメントできます。",
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
    "copy45": "{\n  \"kemi\": 4,\n  \"title\": \"入力の正規化\",\n  \"verdict\": \"changes_requested\",\n  \"approval\": [],\n  \"comments\": [\n    {\n      \"target_type\": \"file\",\n      \"path\": \"src/greeting.ts\",\n      \"side\": \"new\",\n      \"start_line\": 2,\n      \"end_line\": 2,\n      \"body\": \"名前が空なら、ゲストとして扱いませんか？\",\n      \"suggestion\": {\n        \"replacement\": \"  const name = input.trim() || \\\"Guest\\\";\"\n      },\n      \"replies\": [\n        {\n          \"author\": \"agent\",\n          \"body\": \"trim() は前後の空白を取り除きます。空の名前には Guest を使うか、ここで判断してください。\",\n          \"parts\": null\n        }\n      ]\n    }\n  ]\n}",
    "language": "言語",
    "copy46": "日本語の使い方 ↗",
    "copy47": "エージェント用スキル ↗"
  }
};

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
showLanguage(siteLanguage.read('kemi-site-language'));
document.querySelectorAll('[data-language]').forEach(button => {
  button.addEventListener('click', () => {
    const language = button.dataset.language;
    showLanguage(language);
    siteLanguage.save(language);
  });
});
