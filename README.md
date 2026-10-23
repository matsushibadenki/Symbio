# Symbio · Symbolic Intelligence Workbench

<img src="docs/images/symbio-image.png">

Tauri 2 + Rust で動く、意味構造・前提条件・研究履歴を共有する数学ワークベンチの初期実装です。UIからもCLIからも同じRustカーネルを操作します。ThreeXのwgpuレンダラーとATOMのネイティブパネル基盤を流用しています。

**現在はSEG中心のMVPです。Mathematica互換、高度なCAS、LLM、形式証明エンジンは未実装です。**

![Symbio desktop](docs/screenshots/workspace-ja.png)

## 起動 / Run / 启动

macOS開発用アプリを生成します。Rust/Cargo、Xcode Command Line Tools、Python 3が必要です。フロントエンド用のNode/npmビルドは不要です。

```sh
python3 scripts/build-dev-app.py
open target/debug/Symbio.app
```

指定した文書を開く場合（親ディレクトリは先に作成してください）：

```sh
cargo run --locked -p symbio-desktop -- --project /absolute/path/study.symbio
```

文書が存在しなければ多項式と `sqrt(x^2)` のサンプルを作成します。引数なしではTauriのアプリデータディレクトリ内の `workspace.symbio` を使用します。macOSでは通常 `~/Library/Application Support/dev.symbio.workbench/` です。ドック配置はアプリ設定ディレクトリの `dock-layout.json` に保存されます。検証時は環境変数 `SYMBIO_LAYOUT_PATH` で配置を分離できます。

The unsigned macOS development bundle is generated locally. Select **EN / 日本語 / 简体中文** in the toolbar. Expression mode requires explicit multiplication (`2*x`); LaTeX mode also accepts `2x` and previews formulas while editing. Use the checkmark to apply edits, operation buttons to compute, and **⌘S** to save. Windows/Linux builds have not been tested.

这是基于 Tauri 2 与 Rust 的初始数学工作台。工具栏可切换三种语言。普通表达式使用显式乘法（`2*x`）；LaTeX模式支持`2x`并实时预览数式，点击勾选应用编辑，再选择化简、求导、积分或数值计算。**⌘S** 保存文档。尚未集成大语言模型、形式化证明或完整的 Mathematica 兼容功能。

## 使い方

- 左：式を追加・選択し、変数の定義域、非零条件、次元 `[M,L,T]` を設定します。分岐を作ると式と前提条件を独立して編集できます。
- 中央：式、表示数式、計算結果、研究メモ。簡約、微分、定積分・不定積分、行列演算、二次までの方程式、float64数値評価に対応。`⌘Enter` は計算。数値評価の変数値は `{"x":2}` のように入力します。
- 右：入力・出力が参照するSEGノードと操作履歴。履歴には人間／AI、分岐、revision、エンジン名を記録します。初期サンプルの結果もRustで計算しています。
- パネルのタブをドラッグし、中央でタブ結合、四辺で分割、外へ出して別ウインドウ化できます。配置メニューでも同じ操作が可能です。フローティングウインドウを閉じるとパネルが戻ります。
- 分割線はドラッグまたは矢印キーで調整できます。言語・配置・ウインドウ位置を保存します。
- プロットアイコンは独立したwgpuウインドウを開きます。入力式を `x ∈ [-5,5]` の1024点で数値評価し、Rust/GPU内で保持・描画します。変数は `x` のみ。定義できない点は除外され、誤差保証や連続性の保証はありません。
- JSONアイコンで文書全体を表示・選択できます。ファイルとしての保存先指定は現在CLIまたは起動引数で行います。

未保存の式・メモは保存操作で確定します。構文エラーがあれば保存を中止します。未保存状態での終了はブロックされます。分岐切替・Undo・Redo・削除の前に、入力中の式とメモを適用してください。Undo/Redoはセッション内の直近32操作です。

## 数学キーボード / Math keyboard / 数学键盘

数式欄の下のキーボードアイコンで開きます。選択中の式を分数・累乗・根号・絶対値で包み込み、次に入力する引数を選択します。数値・演算子・初等関数のキー、カーソル移動、削除、入力専用Undo/Redoがあります。LaTeX形式の微積分・行列タブは、そのまま計算できるテンプレートを挿入します。`Tab` / `Shift+Tab` で `□` の間を移動できます。

研究メモ内のキーボードでは、提供された `mathkeyboard.html` のキー配列を利用したLaTeXテンプレート（分数、積分、極限、総和、集合など）が「記法集」タブで使えます。計算用パレットは対応する構文に整理し、記法集の未対応記法は明示的なエラーにします。`□` は入力する箇所です。入力形式を **LaTeX** にすると、対応記法をRustでSEGへ解析し、MathMLとしてプレビューします。チェックで確定するか、そのまま微分・簡約・数値評価を実行できます。研究メモではプレビューアイコンで表示し、＋でメモを残したまま別の計算セルを作成します。元サイトやMathJaxへの接続は不要です。入力履歴は文書Undoと独立し、直近32入力までを保持します。

Open the keyboard icon below an expression to insert selection-aware templates. Select LaTeX input mode for live formula preview and SEG-backed calculations. In a research note, preview LaTeX and use + to create a separate computational cell.

点击表达式下方的键盘图标，可插入分数、幂与根号等模板。选择LaTeX输入模式即可实时预览并通过SEG计算。研究笔记可预览LaTeX，点击＋创建独立的计算单元。

### LaTeX入力の対応範囲

入力形式を切り替えるとRustが意味構造を介して変換します。例えば `sqrt(x^2)` は根号を保ったLaTeXになります。積分・行列など通常の式形式で表せない構造は変換を拒否し、元の入力を保持します。

例：`\frac{x^{3}+2x^{2}-5x}{2}` → 数式プレビュー → 微分 → `3/2*x^2 + 2*x - 5/2`。

- 四則演算、分数 `\frac{a}{b}`、根号 `\sqrt{x}`、累乗 `x^{2}`、暗黙の乗算 `2x`、丸／角／波括弧。
- `\sin(x)`、`\cos(x)`、`\exp(x)`、`\ln(x)`、`\log(x)`（自然対数）、`\sin^{2}(x)`、`\left|x\right|`。関数の引数は括弧で囲みます。
- `$...$`、`$$...$$`、`\(...\)`、`\[...\]`、`\left(...\right)`、LaTeXの間隔指定。
- 対応ギリシャ文字は `alpha, beta, gamma, delta, theta, lambda, mu, sigma, omega` の変数へ変換します。例：`\theta` の値は `{"theta":2}` で指定。`xy` は `x*y`。`\pi` は円周率定数、`x_1` は添字付きの変数として扱います。
- 定積分 `\int_{0}^{1}{x^{2}}\,dx` → `1/3`。不定積分 `\int{x^{2}}\,dx` は原始関数と積分定数の注記を返します。多項式、定数、一部のsin/cos/expに対応。一般の有理関数・広義積分は未対応です。
- 微分記号 `\frac{d}{dx}(x^{3})`、`\frac{\partial}{\partial x}(x^{3})`、整数の有限和 `\sum_{i=1}^{5}{i}`・積 `\prod_{i=1}^{5}{i}`（最大256項）。
- 行列 `\begin{bmatrix}1 & 2 \\ 3 & 4\end{bmatrix}`。matrix/pmatrix/bmatrix/Bmatrix/vmatrix、加減乗算、スカラー倍、転置、整数べき、行列式、逆行列。最大8×8、べきは−16〜16。逆行列と3×3以上の行列式は有理数成分に限定します。
- 等式 `x^{2}-5x+6=0` をSEGの関係として保持し、`x = ?` で `{2,3}` を求めます。有理係数・単変数・二次までの実数解が対象。恒等式、複素解、一般連立方程式は未対応です。
- 極限、一般のTeXマクロ、多重積分記号、高階微分記号などは未対応です。
- `□` をすべて埋めてから計算してください。複数文字の指数は `x^{12}` と書きます。1式2048バイト、512トークン、深さ64まで。プレビューは文書を変更せず、計算結果の証明・定義域の保証も行いません。

数式はMathMLで分数・上下限・行列・等式を組版します。入力と結果の余白・サイズを調整し、拡大アイコンで文字サイズを変えられる数式ビューを開けます。長い数式は途中で折り返さず横スクロールします。

## 実装の境界

- SEGは不変ノードの構造共有DAG。構造が同じ式を共有し、数学的等価性とは区別します。
- 厳密な整数・有理数と有限小数の解析に対応。任意精度の浮動小数点、複素数の数値評価、保証付き区間演算はありません。
- `sqrt(x^2)` は実数なら `abs(x)`、非負・正なら `x`。複素数・未指定では元の式を保持します。`x/x` は非零条件なしには消しません。
- 有理係数の単変数多項式（次数32まで）を展開・微分・積分できます。微分は一部の初等関数にも対応します。一般の積分・因数分解・高次方程式求解は未対応です。
- 前提条件を変更するとその分岐の結果を消去します。再計算は操作ボタンで明示的に行います。セル名は表示ラベルであり、他セルへの変数参照としてはまだ解決しません。
- 結果は `symbolic` または `approximate`。`conjecture` はデータ型のみです。`verified` や `certified_numeric` と表示する機能はなく、形式証明・数値誤差保証を偽りません。
- `.symbio` v1は可読JSON。SQLiteコンテナ、内容アドレス付きデータ、完全なイベント再生・分岐マージは今後の課題です。監査履歴は直近256件、Undoスタックは保存されません。
- AI actor付き構造化APIは利用できますが、AI接続・自然言語解釈・MCPサーバーはありません。外部通信はしません。

[構成](docs/architecture.md) · [操作API](docs/math-api.md) · [ロードマップ](docs/roadmap.md) · [流用元](docs/provenance.md) · [検証](docs/validation.md)

## 開発用チェック

```sh
cargo test --locked -p symbio-core -p symbio-cli -p symbio-desktop
cargo clippy --locked -p symbio-core -p symbio-cli -p symbio-desktop --all-targets -- -D warnings
python3 scripts/smoke-cli.py
python3 scripts/smoke-math-keyboard.py
node --check apps/desktop/ui/app.js
node --check apps/desktop/ui/dock.js
node --test scripts/math-keyboard.test.mjs scripts/latex-preview.test.mjs
```

`apps/desktop/ui` を静的サーバーで配信すると、Rustから生成したサンプルの**閲覧専用**プレビューになります。ブラウザ上に代替のJavaScript計算エンジンはありません。
