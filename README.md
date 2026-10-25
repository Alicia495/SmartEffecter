# Guitar Smart Effecter

Windows用のギターエフェクター。Rust / ASIO / Slintで動作するローカルアプリです。

## 起動

このフォルダーの `start.cmd`、または `dist/GuitarSmartEffecter.exe` を開いてください。
Rustなどの開発ツールは、完成したexeの起動には不要です。

1. M-Track SOLOのInput 2にギターを接続し、本体を **Instrument** にします。
2. M-Track SOLOから音を出す場合はヘッドホン／有線スピーカーを同じ機器につなぎ、Direct/USBを **USB** にします。PC側のスピーカーなどへ出す場合は、アプリの設定で出力先を選びます。
3. アプリで **接続**。初期値はInput 2、48 kHz、32 samplesです。
4. ギターを弾きながら、本体の入力ゲインとヘッドホン／スピーカー音量を調整します。アプリの出力は初期値 −12 dBです。

起動だけでは音声処理を開始しません。接続先は「設定」で選べます。M-Audio公式ASIOドライバーが必要です。
入力はASIOです。出力は同じASIO機器、Windowsの既定の出力、個別のWindows出力デバイスから選べます。
開始できない場合はエラーを表示し、別の機器へ勝手に切り替えません。Windowsの既定を選んだ場合はWindows側の既定設定を使用します。

**Windowsの別デバイスへ出すには：**「設定」→「出力」でスピーカーやヘッドホンを選び、「接続」を押します。接続中は一度「停止」してから変更してください。
出力先は名前だけでなく機器IDで保存します。見つからなくなった機器は選び直してください。

## 操作

- エフェクト名をクリックすると編集対象を選びます。下のON/OFFで個別に切り替えます。
- ノブは上下ドラッグ、ホイール、矢印キーで調整し、ダブルクリックで初期値へ。数値欄は直接入力してEnterで反映します。
- AMPはGainとLevelを表示。**EQ**を開くとBass / Mid / Trebleが現れます。
- **IN / OUT**は直近約2秒の入力／処理後波形。**TUNER**では初期状態で出力をミュートします。
- **BYPASS**は全エフェクトをバイパス。入力・出力ゲインとミュートは有効です。
- Clean / Crunch / Leadを選び、**保存**から自分の音色を保存・上書き・名前変更・削除できます。
- **設定**で入力、レート、バッファ、入力ゲイン、波形の表示、明暗テーマを変更できます。音声設定の変更は停止してから再接続します。

処理順は固定です。

`入力ゲイン → Gate → Comp → Drive → Amp/Cab/EQ → Chorus → Delay → Reverb → 出力ゲイン`

DriveはOverdrive / Distortion、AmpはClean / Crunchを選べます。DelayにはTapがあります。
Cabは軽量フィルター方式です。実機アンプの厳密な再現、IR読込、録音、VST、MIDI、ルーパーは含みません。

## 遅延と安定性

M-Track SOLOで16 / 32 / 64 samplesの起動を確認しています。設定画面のバッファ一覧はドライバーの範囲から作った**候補**で、接続時に可否を確認します。

| 48 kHz | ASIOタイムスタンプによる推定往復遅延 |
|---|---:|
| 16 samples | 約7.48 ms |
| 32 samples | 約6.06〜8.15 ms |
| 64 samples | 約9.48 ms |

Windows出力では、RealtekスピーカーとLGモニターで約48 msの推定値を確認しました。ドライバーの報告値は状態によって変動します。

これらは電気的なループバック実測ではありません。5 ms以下の実測目標は達成確認できていません。
32 samplesを初期値にしています。最小遅延を試す場合は16、ノイズや途切れが出る場合は64または128へ変更してください。
上の数値は同じ機器のASIO出力を使った場合です。Windows出力はWASAPI共有モードを使い、入出力のレート差とクロック差を小さなキュー・補間で補正するため遅延が増えます。Windows出力のバッファは自動設定され、画面のバッファ選択はASIO入力側に適用されます。
「期限超過」はアプリの処理時間が1バッファ時間を超えた回数で、聴感上の音切れ回数とは区別します。
機器が停止したときはアプリも停止し、接続し直せます。最新の検証条件は `TEST_REPORT.md` を参照してください。

## 保存先

`%LOCALAPPDATA%\GuitarSmartEffecter\settings.json`

前回の音色、機器設定、ユーザープリセットを自動保存します。読めない設定は日時付きファイルへ退避してから初期設定で起動します。
起動エラーの詳細は同じフォルダーの `last-error.txt` / `last-panic.txt` に保存します。
テスト用に環境変数 `GUITAR_EFFECTER_DATA_DIR` で保存先を分離できます。
通常の演奏にログインやインターネット接続は不要です。「公式ドライバー」や情報画面のSlintリンクはブラウザーを開きます。

## 開発

Windows x64、Rust MSVC、MSVC C++ツール、Windows SDK、libclang、ASIO SDKを使用します。
この作業環境では開発ツールを `.tools` に配置済みです。`scripts/dev-env.ps1` がそのプロセスの環境だけを設定します。

```powershell
. .\scripts\dev-env.ps1
cargo test --locked
cargo clippy --all-targets -- -D warnings
.\scripts\build.ps1
```

このPCのWindows App Controlでは一部のrelease依存ビルドスクリプトが拒否されました。
そのため配布ビルドは `cargo rustc --locked --bin guitar-smart-effecter -- -C opt-level=3` を使用しています。
アプリ・DSPのクレートは最適化済み、依存クレートはdevプロファイルです。セキュリティ設定は変更していません。
別の開発環境では通常の `cargo build --release --locked` も利用できます。

主なソースは `src/audio.rs`（ASIOとコールバック）、`src/dsp.rs`（DSP）、`src/state.rs`（設定とパラメーター）、`src/app.rs`（UI連携）、`ui/app.slint`（画面）です。

## 診断

exeはGUIアプリのため、PowerShellでは `Start-Process -Wait` を使い、結果をファイルへ出力してください。

```powershell
Start-Process .\dist\GuitarSmartEffecter.exe -ArgumentList '--devices --report devices.json' -Wait
Start-Process .\dist\GuitarSmartEffecter.exe -ArgumentList '--outputs --report outputs.json' -Wait
Start-Process .\dist\GuitarSmartEffecter.exe -ArgumentList '--probe 30 --buffer 32 --all-effects --stress-controls --report probe.json' -Wait
Start-Process .\dist\GuitarSmartEffecter.exe -ArgumentList '--probe 5 --output windows:default --report windows-probe.json' -Wait
Start-Process .\dist\GuitarSmartEffecter.exe -ArgumentList '--demo' -Wait
```

`--probe`は実機入出力を開きますが、出力はミュートします。
`--live-test 1800 --buffer 32 --report soak.json` は波形とUI操作を伴う30分テストで、こちらもミュートです。
`--demo`は合成信号のデザイン確認専用で、機器に音を出しません。

依存関係のライセンス情報は配布フォルダーの `THIRD_PARTY_NOTICES.md` と `licenses` に含みます。


## AG03MK2・簡易ミキサー・ベース（2026-10-25）

オーディオ設定で Yamaha Steinberg USB ASIO の機器を選択します。AG03MK2 本体の STREAMING OUT は **DRY CH 1-2G**、楽器は Input 2、同じ本体のマイクは Input 1 を選びます。LOOPBACK はアプリ出力を再入力するため使わないでください。二重モニターを避ける場合は本体 MIX MINUS を利用します。
公式資料: https://manual.yamaha.com/pa/live_streaming/ag03_06/en-US/8410785419.html

「マイク」は、なし／同じASIO機器の別入力／Windowsの入力機器から選択。マイク入力CHは1から数えます（通常のUSBマイクは1）。接続中は入力機器を変更できません。
楽器音量とマイク音量のみの簡易ミキサーです。楽器のエフェクト後に加工しないマイクを加算し、選んだ一つの出力先へ送ります。MUTE は両方に適用。音量は -60～+6 dB、合成時の過大出力はクリップ表示と最終クランプで扱います。楽器の既存OUTノブは楽器経路の音量です。仮想出力は下記のVB-CABLE経路を利用します。

低遅延には同じ機器のASIO入出力を使い、48 kHz / 32～64 samples から調整します。別USBマイクはWASAPI入力とクロック補正用バッファを使用するためマイク側に追加遅延があります。楽器をマイクに合わせて待たせる処理はありません。

「ベース用」で低域向けEQ・低域カット周波数と、歪みに低域原音を混ぜる処理に切り替わります。「Bass Clean」「Bass Drive」でコンプを含む初期音色を呼び出せます。切替と音色は保存対象です。

AG03MK2実機・別マイクとの同時入力・電気的ループバック遅延は未検証です。テスト結果は TEST_REPORT.md を参照してください。


### 2026-10-25 追加修正

- ベースのチューナーを低いB弦（B0）まで拡張。
- マイクのMUTEと接続開始時の音量を滑らかに変更。
- 別マイクの入力ブロックに合わせて同期キューの最低量を確保。楽器経路の待ち時間は増やしません。
- 診断欄に別マイクのキュー時間・欠落・あふれ・エラーを表示。マイクエラー時は楽器を継続してメッセージを表示。
- マイクCHの数値入力はEnterで確定（入力途中で値を書き戻す問題を修正）。


## 仮想出力（2026-10-26）

楽器のエフェクト音とマイクを、OBS・Discord等のマイク入力として渡せます。

1. https://vb-audio.com/Cable/ からVB-CABLEのWindows用公式ドライバーを導入します。管理者権限が必要です。再起動が要求された場合は再起動します。
2. 本アプリの「設定」→「再検出」→「仮想出力を使う」を選び、「接続」。
3. 受け側アプリのマイク／音声入力に **CABLE Output (VB-Audio Virtual Cable)** を選びます。

音の流れ: `ASIO楽器 → エフェクト → マイクと合成 → CABLE Input → CABLE Output → 受け側アプリ`。
仮想出力とは別に、下記のベース専用ASIOモニターを利用できます。
VB-CABLEは別途導入する外部ドライバーで、このアプリが独自のドライバーを作成・同梱するわけではありません。

仮想出力は保存可能な専用モードです。未導入・無効・候補が複数ある場合は開始エラーになり、Windows既定や物理スピーカーへ切り替えません。複数候補時は出力一覧から目的のCABLE Inputを直接指定できます。
「仮想出力の設定」は公式導入ページを開きます。アプリの自動操作でドライバーのインストールや再起動は行いません。
別の仮想ケーブルやVoiceMeeterを使用する場合は、Windows出力一覧から該当の再生側デバイスを直接選択できます。

低遅延には48 kHzで送受信側の形式を揃え、楽器のASIOバッファは32～64 samplesから調整します。仮想出力はWASAPIの同期キュー、VB-CABLE、受け側アプリのバッファが加わるため、同一機器ASIOより遅延します。アプリの推定往復遅延には、VB-CABLE内部と受け側アプリの遅延を含みません。
検証用CLI: `GuitarSmartEffecter.exe --probe 5 --output virtual:vb-cable --report artifacts/virtual-probe.json`（出力はミュート）。


## 自分にはベースだけ返す（2026-10-26）

「設定」で仮想出力を選び、**自分にはベースだけ返す** をオンにします（既定オン）。「自分用の出力」でAG03MK2等の入力機器のASIO出力、Windowsの既定出力、個別のヘッドホン／スピーカーから選びます。

- Discord行き: ベース → エフェクト → 楽器音量 → PCマイクと合成 → 仮想出力。
- 自分行き: 同じエフェクト後のベース → 楽器音量 → モニター音量 → 選択した自分用の出力。PCマイクは含めません。

モニター音量はDiscord行きに影響しません。MUTEとチューナーのミュートは両方へ適用します。
ASIO選択時はモニター専用の短いキューを使い、Windows出力選択時は別の同期キューを使うため、どちらも仮想出力側のバッファを待ちません。仮想出力が滞留してもモニター処理をブロックしません。
送信用・自分用とも同じASIO機器を選んだ場合は通常のミックス出力のみとなります。自分用に別のWindows機器を選べば、送信先がASIOの場合もベースだけのモニターを利用できます。
AG03MK2等の本体側ダイレクト音が別途聞こえる場合、本体側のモニター設定も調整してください。


### 自分用の出力先の変更

接続停止中に「自分用の出力」で選び、再接続します。選択は機器IDで保存します。未接続の機器は一覧に残し、勝手に既定機器へ切り替えません。「Windowsの既定の出力」はWindowsの設定に従います。
同じ機器のASIO出力が最小遅延です。別機器では専用の同期キューとリサンプルを利用し、その機器のバッファ分の遅延が加わります。Bluetoothヘッドホン等の伝送遅延はアプリの推定値に含まれません。
送信用と自分用に同じWindows出力機器を指定すると、ベースだけを分離できないため開始時にエラーを表示します。
検証用CLI: `--probe 5 --output <送信用ID> --monitor-output <自分用ID>`（音声出力はミュート）。


### マイクの一時欠落と接続エラー

WASAPIの一時的なデータ不連続（Xrun）は診断の「一時欠落」に数えます。ストリームが続いている状態で接続失敗とは表示しません。切断・アクセス拒否・形式変更等は別のエラーとして表示します。音切れ自体を解消したと見なすものではありません。
実機検証のCLIには `--mic <マイク機器ID>` と `--mic-channel 1` を指定できます。入力機器IDの一覧は `--inputs`。
VB-CABLEの標準出力モードは、16ch版と標準版が並存していても標準のCABLE Inputを選びます。


## 和音時のざらつき対策（2026-10-26）

DRIVEとAMPの非線形処理に一次のantiderivative antialiasing（ADAA）を導入しました。サンプル間の非線形伝達を積分平均し、歪みによる高域の折り返し成分を抑えます。差分が小さい場合は中点で評価し、積分計算はf64で行います。固定サイズの状態だけを使い、追加のブロック待機はありません。非線形経路の位相・高域応答は以前と少し変わります。
ベースモードのDRIVEは低域を原音側へ分離してから歪ませるため、低域同士が歪みを通じて干渉する濁りを抑える構成です。Bass Driveの音色・音量は以前と少し変わります。

強い歪み自体が作る和音の相互変調はなくなりません。ベースの和音を明瞭にしたい場合はBass Cleanから調整し、必要に応じてDriveのGainを上げ、Chorus/Delay/Reverbを追加してください。既存の音色や保存設定を自動で変更することはありません。
合成テスト音の生成: `--render-chords --out artifacts/chords --report artifacts/chords.json`。現在の音色・Bass Clean・Bass Driveそれぞれについて、単音2秒→和音2秒→余韻2秒のWAVを出力します。
