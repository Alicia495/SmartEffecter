# 初版の検証結果

2026-10-17 / Windows x64 / M-Audio M-Track SOLO / 48 kHz

## 実装・操作

- Rustの単体テスト17件成功。Gate、Comp、Cab、Chorus、Delay、Reverb、チューナー、ミュート／バイパス、設定を検証。
- ASIO入力のInput 2ルーティング、自作コールバックでのメモリ確保なしを確認。
- Windows出力の48 kHz↔44.1 kHz変換、独立クロックのずれ、モノラル出力、データ不足からの復帰を検証。補正経路もメモリ確保なし。
- ネイティブUIでエフェクト選択とON/OFF、ノブのドラッグ／矢印キー、数値入力、ミュート、EQ展開、Windows出力先の保存、プリセット保存・名前変更・削除を確認。
- 通常／最小サイズ、明暗テーマ、設定、チューナーをネイティブ画面のスナップショットで確認。

## 短時間の実機確認

すべて出力をミュートし、全エフェクトを有効化。実際のギター演奏の聴感試験ではない。

| 経路 | 時間 | 推定往復遅延 | 欠落／オーバーフロー／エラー／処理期限超過 |
|---|---:|---:|---|
| M-Track SOLO ASIO入力 → 同機器ASIO出力、32 samples | 4秒 | 6.06 ms | すべて0 |
| M-Track SOLO ASIO入力 → Windows既定（Realtekスピーカー） | 5秒 | 47.84 ms | すべて0 |
| M-Track SOLO ASIO入力 → 個別選択したLG ULTRAWIDE | 4秒 | 48.02 ms | すべて0 |
| M-Track SOLO 44.1 kHz入力 → Windows既定出力 | 4秒 | 48.34 ms | すべて0 |

遅延はコールバックのタイムスタンプとキュー量からの**推定**。電気的な往復実測ではない。
ドライバーの状態によって報告値は変動する。同機器ASIOの先行確認では32 samplesが8.15 ms、16 samplesが7.48 ms、64 samplesが9.48 msだった。
Windows出力には共有モードのドライバー遅延と約13 msの同期キューが加わる。最小遅延を優先するときは同機器ASIO出力を使う。

## 長時間試験の扱い

30分の実機試験は、ユーザーの指示によりスキップした。
停止前の記録は約659秒、32 samples、全エフェクト、波形表示・UI操作ありで、欠落・オーバーフロー・ドライバーエラー・処理期限超過はいずれも0。
この途中記録を30分合格とは扱わない。

先行試験で小数ディレイの丸めによる範囲外参照を検出し、整数インデックスの折り返しに修正。境界値の回帰テストを追加済み。
修正後、合成信号と無音・パラメーター変更を含む30分相当のDSPデータ（86,400,000フレーム）を約14.81秒でオフライン処理し、異常値・クラッシュなし。
このオフライン試験は実時間でのハードウェア安定性試験とは別。

## 未確認の事項

- 実際にギターを弾いた音質・クリックノイズ・聴感上の応答。
- 電気的ループバックによる実測遅延。実測5 ms以下は未達成扱い。
- 物理的なUSB抜き差し、列挙されたすべてのWindows機器、Bluetooth機器。

## 記録

`logs/tests.log`、`logs/clippy.log`、`artifacts/ui-smoke.json`、
`artifacts/probe-asio-final.json`、`artifacts/probe-windows-final.json`、
`artifacts/probe-windows-monitor.json`、`artifacts/soak-32-stopped-by-request.json`、
`artifacts/dsp-stress-30min.json`。


## 2026-10-25: AG03MK2入力・ミキサー・ベース拡張

- cargo test --locked: 18件成功。
- cargo clippy --all-targets --locked -- -D warnings: 成功。
- 同じASIO機器の2入力を異なる音量で合成し、期待値と全体MUTEを確認。
- 別マイクを模した44.1 kHz入力を48 kHz楽器コールバック内でリサンプル・合成して期待値を確認。
- 上記2経路のコールバックでヒープ確保0件。
- Bass Clean / Driveの30.87 Hz入力は有限値を維持し、ギター設定より低域を保持。ヒープ確保0件。
- UIスモーク: ベース音色切替、ミキサー音量、マイク入力設定の保存と既存操作を検証。設定画面のPNGを目視確認。
- 現在のASIO列挙結果はM-Trackのみ。AG03MK2は公式仕様の独立CHモードに合わせた実装で、実機接続・別USBマイクの実録音・長時間同期・電気的往復遅延は未検証。
- 配布用exeを再ビルド。以前の6.06 ms等は今回の追加経路の測定結果ではない。


## 2026-10-25 追加点検・修正

- cargo test --locked: 21件成功。clippy全ターゲット警告0。
- ベース5弦のB0/E1/A1/D2/G2を44.1/48 kHz由来のチューナー入力で検証（2 cents以内）。
- マイクのミュート開始時に急な振幅ステップがなく、最終的に無音になることを検証。
- 20ms周期のマイク入力→32 samplesの処理を10秒分シミュレートし、欠落0・あふれ0・コールバックのヒープ確保0を確認。
- ミキサーとbassフィールドのない旧形式設定が既定値で読めることを確認。
- マイクCH入力はEnter確定へ変更。入力中の文字列を毎回書き戻さない。
- マイククリップのメーター反映、マイク診断・エラー表示を追加。
- 実機のマイク抜き差し、AG03MK2と別USBマイクの組合せ、電気的遅延は未確認。


## 2026-10-26 仮想出力

- VB-CABLE用の専用出力モード（virtual:vb-cable）を追加。導入済みのWASAPI再生エンドポイントを解決して合成音を送信。
- cargo test --locked: 22件成功。clippy全ターゲット警告0。最適化ビルド・配布exe更新済み。
- 未導入・誤方向のCABLE Output・複数候補を対象に、物理/既定出力へフォールバックしないことを単体テストで確認。
- UIスモーク: 仮想出力ボタン・設定保存と読込・既存の操作が成功。設定画面PNGを目視確認。
- 実機の出力列挙で「仮想出力 · VB-CABLE（要セットアップ）」を確認。
- 配布exeの --probe 1 --output virtual:vb-cable は未導入を示す説明付きエラーで終了（exit 1）。音声ストリーム開始前に検出。
- 本PCにVB-CABLE未導入のため、CABLE Outputからの受信・通話/録音アプリでの受信・経路全体の実測遅延は未確認。


## 2026-10-26 ベース専用ASIOモニター

- 仮想/Windows出力のミックスとは別に、エフェクト後の楽器だけを入力機器のASIO出力へ送るモニターを追加。
- モニターは独立した短いSPSCキュー（指定ASIOバッファの4倍、最低64 frames）。仮想出力と独立して消費し、送信失敗でもブロックしない。
- 同じコールバックで送信ミックス0.325に対してモニター0.1となる既知信号の検証を追加し、マイク非混入を確認。コールバックのヒープ確保0件。
- cargo test --locked: 22件成功。clippy全ターゲット警告0。
- 新配布exeでUIスモーク成功。モニター有効・音量の保存/読込確認。画面PNGを目視確認。
- M-Track実機の48 kHz / 32 samplesでWindows既定出力＋ASIOモニターの同時出力を5秒間実行。出力はミュート。各経路とも欠落0、あふれ0、エラー0、音声処理期限超過0。
- 根拠: artifacts/bass-monitor-hardware-probe-v2.json。monitor_metricsのdriver_output_msはASIO出力側のみで、往復遅延の実測値ではない。
- VB-CABLE未導入のためDiscord受信との同時動作・AG03MK2実機・実際の演奏音と電気的往復遅延は未確認。
- 起動中の旧exeはdist/GuitarSmartEffecter.previous-20261026-000755.exeへ退避。新exeとtarget/debug/guitar-smart-effecter.exeのSHA256一致を確認。起動中のアプリは旧版のままで、反映には再起動が必要。


## 2026-10-26 自分用出力の選択・マイク警告の修正

- 自分用の出力先をASIO、Windows既定、個別WASAPI機器から選択可能にした。ASIOは短い専用キュー、別機器は独立したRateBridgeを利用。マイクを混ぜる前のベース信号を返す。
- 出力先を機器IDで保存。未接続時はエラー表示、送信用と同じWindowsエンドポイントを選ぶ場合も開始前にエラー。
- CPAL 0.18.2のWASAPI入力はDATA_DISCONTINUITY時にErrorKind::Xrunを通知する。旧コードが全通知を接続エラーとしてカウントし、UIが累積値を見て警告し続ける問題を修正。Xrunは別カウンター、切断や権限拒否等はエラー扱いを維持。
- 起動中exeを上書きせず、退避して新exeを配置する配布スクリプトに修正。起動中アプリの強制終了はしない。
- cargo test --locked: 23件成功。clippy全ターゲット警告0。配布ビルド・依存ライセンス生成成功。
- UIスモークと画像確認成功。自分用出力Windows既定・音量の保存/復元を確認。旧設定にmonitor_deviceがなくてもASIOを既定として読込可能。
- 現在の実機: M-Track、Anker PowerCast M300、VB-CABLE、final_UX5000。48 kHz / 64 samplesで仮想出力＋マイク＋Windows既定モニターを10秒、同じヘッドホンの個別ID指定を5秒確認（出力はミュート）。
- 両試行ともマイクのXrunを1回再現。マイク接続エラー0、マイクキュー欠落/あふれ0、送信・モニターの欠落/あふれ/エラー0。処理期限超過0。音切れ通知が接続エラーとされないことを確認。
- 根拠: artifacts/mic-virtual-windows-monitor-probe.json、artifacts/mic-specific-monitor-probe.json。
- 同一Windows出力を送信/モニターに指定するCLI検査は、説明付きエラーで終了（exit 1）。
- VB-CABLEはユーザー側で導入済みとなり、標準CABLE Inputと16ch版が並存。標準版を選ぶよう自動判別を修正しテスト。
- Discord側の受信音・実際の演奏音・AG03MK2実機・電気的遅延は未確認。今回の実機試行はミュートしたストリームの動作確認。


## 2026-10-26 和音・歪み処理の点検

- 保存音色のスナップショットはbass=false、Drive Gain 65、Amp Crunch Gain 42、Chorus/Delay/Reverb有効だった。ユーザーから自分用とDiscord双方でざらつくとの報告。
- 同一設定の合成単音→和音→余韻を修正前後でレンダー。各6秒、現在の音色/Bass Clean/Bass Driveとも有限値で最終クリップ0。ただし実演奏のADC/ミキサーでのクリップを否定する証拠ではない。
- DRIVE/AMPに一次ADAAを導入。f64積分差分と小差分時の中点評価。ベースDriveは低域を原音として残し、高域側を歪ませる。
- 9 kHz入力・48 kHz時の折り返し3 kHz成分は、Drive 2モード/AMP 2モードとも旧処理の半分未満のパワーに低下するテストが成功。この特定の試験条件の結果であり、全ての和音成分が半減する意味ではない。
- 非線形積分の微分一致、ほぼDCでの数値安定性、コールバックのヒープ確保0、既存のバイパス/ミュート/低域保持/キャビネットのテストが成功。Bass Driveの低域保持は増幅の異なるギターモードでなくClean Bassを基準に比較するよう検証条件を更新。
- cargo test --locked: 25件成功。clippy全ターゲット警告0。最適化した配布exeとUIスモーク成功。
- M-Track / Anker / VB-CABLE / Windowsモニター、48 kHz / 64 samples、全エフェクト有効＋パラメーター連続変更を10秒間確認（ミュート出力）。最大処理145us、期限超過0、送信/モニターの欠落・あふれ・エラー0、マイク接続エラー0。
- 根拠: artifacts/chords-before.json、artifacts/chords-after.json、artifacts/chord-dsp-hardware-probe.json。比較WAV: artifacts/chords-before/ と artifacts/chords-after/。beforeは開発ビルド、afterは最適化ビルドなのでレンダー速度の直接比較はしない。
- 実演奏音による聴感比較、Discordでの受信音、電気的遅延は未確認。相互変調を完全に除去する修正ではない。保存音色・実ユーザーの設定は変更していない。
