# SmartEffecter

Windows用のギター・ベースエフェクター。PCのマイクと合成してDiscordに送り、自分には楽器の音だけを返せます。

## ダウンロード

[**v1.0をダウンロード**](https://github.com/Alicia495/SmartEffecter/releases/tag/v1.0)

`SmartEffecter-v1.0-windows-x64.zip` を展開し、`GuitarSmartEffecter.exe` を起動してください。インストール不要です。
Windows 10 / 11（x64）と、入力機器のASIOドライバーが必要です。

## 使い方

1. ギター／ベースをオーディオ機器につなぐ。
2. **設定**で入力機器とチャンネルを選び、**接続**する。
3. 好きな音色を選んで調整する。ベースは **Bass Clean / Bass Drive** から始められます。

### Discordに送る

[VB-CABLE](https://vb-audio.com/Cable/)を導入し、次のように設定します。

| 項目 | 設定 |
|---|---|
| アプリのマイク | PCにつないだマイク |
| アプリの出力 | 仮想出力 · VB-CABLE |
| Discordの入力 | CABLE Output |
| 自分用の出力 | ヘッドホンなど、聴きたい機器 |

**「自分にはベースだけ返す」**をオンにすると、マイクを混ぜる前の楽器音をモニターできます。

## できること

- Gate / Comp / Drive / Amp・Cab / Chorus / Delay / Reverb
- ベース向けの音色、チューナー、波形表示、プリセット保存
- 楽器とマイクの音量調整、仮想出力、楽器だけのモニター

低遅延を優先するなら、入力と同じ機器のASIO出力を使ってください。別機器やBluetoothでは遅延が増えます。

[接続・トラブル対処](docs/SETUP.md) · [ビルド方法](docs/BUILDING.md) · [変更履歴](CHANGELOG.md) · [検証結果](TEST_REPORT.md)

## ライセンス

[GPLv3](LICENSE)。配布ZIPには依存ライブラリのライセンスも同梱しています。
