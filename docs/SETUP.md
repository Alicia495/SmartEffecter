# 接続とトラブル対処

## 必要なもの

- Windows 10 / 11 x64、オーディオ機器の公式ASIOドライバー。
- Discordなどへ音を渡す場合は[VB-CABLE](https://vb-audio.com/Cable/)。ドライバーは同梱していません。
- `VCRUNTIME140.dll` が見つからない場合は、[Microsoft Visual C++ Redistributable x64](https://aka.ms/vc14/vc_redist.x64.exe)を導入してください。

## 入力機器

### Yamaha AG03MK2

本体の **STREAMING OUT = DRY CH 1-2G**、アプリの楽器入力は **Input 2**。本体のマイクを使うなら、アプリのマイクで「同じASIO機器の別入力」、CH 1を選びます。
LOOPBACKはアプリの出力が戻るため使いません。本体のダイレクト音が重なる場合はMIX MINUSなどのモニター設定を調整してください。
[ヤマハ公式マニュアル](https://manual.yamaha.com/pa/live_streaming/ag03_06/en-US/8410785419.html)

### M-Audio M-Track SOLO

楽器をInput 2につなぎ、本体をInstrumentに設定します。本体へモニターする場合はDirect/USBをUSBにします。
[公式ドライバー](https://www.m-audio.com/support/downloads)

## Discordと自分用のモニター

```text
楽器 → エフェクト ─┬→ PCマイクと合成 → VB-CABLE → Discord
                  └→ モニター音量 → 自分のヘッドホン
```

1. アプリの設定で入力機器と楽器チャンネルを選びます。
2. 「マイク」でPCのマイクを選びます。通常はマイクCH 1（Enterで確定）です。
3. 「仮想出力を使う」を選びます。
4. 「自分にはベースだけ返す」をオンにし、「自分用の出力」を選びます。
5. 接続後、楽器・マイク・モニターの音量を調整します。
6. Discordの入力デバイスを **CABLE Output** にします。

モニター音量はDiscord行きに影響しません。MUTEとチューナーのミュートは両方に適用します。
出力先・入力先の変更は停止中に行ってください。送信用と自分用に同じWindows出力を選ぶと開始エラーになります。

## 音が出ない・途切れる

- ASIO機器とチャンネル、MUTE、音量、本体の設定を確認し「再検出」→再接続します。
- VB-CABLE導入後は、必要に応じてWindowsを再起動します。未導入時に物理スピーカーへ自動で切り替えません。
- 48 kHz / 64 samplesから試し、安定すれば32へ、途切れる場合は128へ調整します。
- 別出力には同期バッファが加わります。Bluetoothの伝送遅延はアプリの推定値に含まれません。
- Discordだけで音が欠ける場合は、ノイズ抑制や自動ゲイン調整を切り替えて比較してください。
- マイクの「一時欠落」は短いデータ不連続の通知です。切断・権限拒否とは別に表示します。
- 和音が濁る場合はBass Cleanから調整し、DriveやAmpのGain、Chorus等の量を下げて比較します。強い歪み自体による濁りは残ります。

## 設定と更新

設定は `%LOCALAPPDATA%\GuitarSmartEffecter\settings.json` に自動保存します。更新時はアプリを終了して新しいZIPを別フォルダーへ展開し、起動してください。通常は既存設定を引き継ぎます。
読めない設定は退避して初期設定で起動します。起動エラーは同じ場所の `last-error.txt` / `last-panic.txt` に保存します。

## 検証の範囲

M-Track、Ankerマイク、VB-CABLE、Windowsモニターの同時ストリームを確認済みです。AG03MK2実機、Discordの受信音、実演奏の聴感、電気的な往復遅延は未確認です。詳しくは[検証結果](../TEST_REPORT.md)を参照してください。
