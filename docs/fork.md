# fork（ryo246912/nootle）の配布・運用

このリポジトリは [michellemayes/nootle](https://github.com/michellemayes/nootle)（以下 upstream）の fork で、macOS（Apple Silicon）向けの DMG 配布、Homebrew Cask、アプリ内の自動更新を **すべて fork 側の GitHub Releases** から行う。upstream の機能追加やバグ修正は定期的にマージして取り込む。

## インストール

```bash
brew tap ryo246912/nootle https://github.com/ryo246912/nootle
brew install --cask nootle
```

mise で管理する場合（`mise bootstrap packages brew tap` は `[bootstrap.brew.taps]` を書き換えるコマンド）:

```bash
mise bootstrap packages brew tap ryo246912/nootle https://github.com/ryo246912/nootle
mise bootstrap packages apply brew-cask:ryo246912/nootle/nootle
```

```toml
[bootstrap.brew.taps]
"ryo246912/nootle" = "https://github.com/ryo246912/nootle"

[bootstrap.packages]
"brew-cask:ryo246912/nootle/nootle" = "latest"
```

tap の URL を明示するのは、mise が既定で `homebrew-<tap>` という名前のリポジトリを探すため。

### 更新の経路

| 経路 | 参照先 |
| --- | --- |
| アプリ内の自動更新 | `src-tauri/tauri.conf.json` の `plugins.updater.endpoints`（fork の `latest.json`）。署名は fork 専用の鍵（下記）で検証する |
| `brew upgrade` / `mise bootstrap packages upgrade` | `Casks/nootle.rb`（fork の DMG） |

Cask は `auto_updates true` なので、brew と mise は初回に現行版を入れるだけで、以後はアプリ自身の更新に任せる。mise の `upgrade` は、入っているアプリが Cask より古く、かつ起動していないときだけ差し替える。

upstream 版へ意図せず更新されないのは、次の 3 つが重なっているから。

1. 更新の取得先が fork の `latest.json` だけ
2. 更新の署名検証に fork 専用の公開鍵を使う（upstream の鍵で署名された更新は受け付けない）
3. リリースワークフローが、endpoint と公開鍵が upstream のままなら失敗する（`scripts/fork/check-updater-config.mjs`）。upstream をマージして設定が戻っても、そのままでは配布されない

### dotfiles への組み込み

`ryo246912/dotfiles` の `config-mac/mise/config.mac.toml` に Nootle の設定は無い（2026-11-23 時点）ので、置き換えではなく追加になる。最初のリリースが公開され、Cask が fork の DMG を指してから適用する。

```diff
@@ [bootstrap.packages]
 "brew-cask:rectangle"                 = "latest" # 1.100
+# fork 版 Nootle。upstream ではなく ryo246912/nootle の Releases を配布元・自動更新元にする。独自 tap のためバージョン注記は付けない
+"brew-cask:ryo246912/nootle/nootle"   = "latest"
 "brew-cask:slack"                     = "latest" # 4.51.180
@@ [bootstrap.brew.taps]
 "kamillobinski/thock" = "https://github.com/kamillobinski/homebrew-thock.git"
+"ryo246912/nootle"    = "https://github.com/ryo246912/nootle"
```

すでに upstream 版（`michellemayes/nootle`）を入れていた場合は、先に `brew uninstall --cask nootle` と `brew untap michellemayes/nootle` で外す。同名の Cask トークン `nootle` が 2 つの tap に存在すると、`brew install --cask nootle` がどちらを指すか曖昧になる。fully-qualified な `ryo246912/nootle/nootle` なら曖昧にならない。

## 初回セットアップ（一度だけ）

1. **Actions を有効にする**。fork では最初、workflow が無効になっている。Actions タブで有効化する。
2. **upstream のタグを fork に押し上げる**。リリースのバージョンは「最新の `v*` タグ + コミット履歴」から決まる。タグが無いと 0.x の頭から数え直してしまう。
   ```bash
   git fetch upstream --tags
   git push origin --tags
   ```
3. **更新用の署名鍵を作る**。
   ```bash
   scripts/fork/setup-updater-key.sh
   ```
   鍵を生成し、公開鍵を `src-tauri/tauri.conf.json` に書き込み、秘密鍵を Actions secrets（`TAURI_SIGNING_PRIVATE_KEY`、パスワードを付けた場合は `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`）に登録する。`tauri.conf.json` の変更はコミットして main に入れる。秘密鍵（既定は `~/.tauri/nootle-fork.key`）は必ずバックアップする。失うと、すでに入っているアプリは新しい更新を検証できなくなる。
4. **（任意）Apple の署名・公証**。下の「署名と公証」を参照。無くてもリリースできる。
5. 上の変更を main に入れると Publish が走り、最初の fork リリースができる。完了後、リリースの DMG と `.sha256` を確認し、`brew tap` と `brew install --cask nootle` を試す。

最初のリリースまでは `Casks/nootle.rb` の `version` / `sha256` が upstream の v0.37.1 のままで、fork 側にその DMG は無い。リリースが出るまでは `brew install` できない。

## fork のリリース手順

通常は何もしなくてよい。**main に push（マージ）すると、Publish ワークフローが自動でリリースする**。

1. 前回の `v*` タグ以降の conventional commits から、git-cliff が次のバージョンを決める（`feat:` で minor、それ以外は patch。ルールは `cliff.toml` の `[bump]`）。
2. `aarch64-apple-darwin` 向けに DMG を作り、draft の GitHub Release に `Nootle_<version>_aarch64.dmg`、更新用の `.app.tar.gz` と `.sig`、`latest.json` をアップロードする。
3. テストとビルドの両方が通ったら、`Nootle_<version>_aarch64.dmg.sha256` を追加し、draft を公開する。公開時にタグ `v<version>` が付く。
4. `CHANGELOG.md` を再生成し、`Casks/nootle.rb` の `version` と `sha256` を書き換えて main に push する（`chore: update changelog for v<version>`）。この push では再リリースされない（`Casks/**` は paths-ignore）。

手動で動かすときは Actions → Publish → Run workflow。ビルドが落ちた場合は draft が自動で消えるので、原因を直して再実行すればよい。

`docs/**` と `**.md` だけの変更ではリリースされない。

### DMG のチェックサムを確かめる

```bash
shasum -a 256 -c Nootle_<version>_aarch64.dmg.sha256
```

## 署名と公証

fork のリリースは、Apple の資格情報が無くても作れる。ただし、その場合は次の制約がある。

| | 資格情報なし（既定） | Developer ID + 公証あり |
| --- | --- | --- |
| 署名 | ad-hoc（`-`） | Developer ID Application |
| Gatekeeper | 警告で止まる。Cask が隔離属性を外すので brew / mise 経由なら開ける | そのまま開ける |
| DMG を手でダウンロード | 初回は右クリック →「開く」、または `xattr -dr com.apple.quarantine /Applications/Nootle.app` が要る | そのまま開ける |
| マイク・画面収録などの許可 | **更新のたびに付与し直しになる可能性が高い** | 維持される |
| 更新の検証 | Tauri の署名鍵（Apple とは別）で検証。どちらでも同じ | 同じ |

ad-hoc 署名の指定要件は、ビルドごとに変わるコードのハッシュに基づく。macOS の許可（TCC）はこの要件に紐づくため、アプリ内更新や mise / brew による差し替えのたびに、マイク・画面収録・カレンダーなどの許可が外れることがある。Developer ID で署名すると、証明書に基づく要件になり維持される。upstream は後者。

### 公証を有効にする条件

Apple Developer Program（有料、年額）への加入が必要。次の secrets を Actions に登録する。`APPLE_CERTIFICATE` があれば署名を、`APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID` の 3 つがそろえば公証も有効にする。

| secret | 内容 |
| --- | --- |
| `APPLE_CERTIFICATE` | Developer ID Application 証明書（.p12）を base64 にしたもの |
| `APPLE_CERTIFICATE_PASSWORD` | .p12 のパスワード |
| `KEYCHAIN_PASSWORD` | ビルド用の一時キーチェーンのパスワード（任意の文字列） |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: 名前 (TEAMID)` |
| `APPLE_ID` | Apple ID のメールアドレス |
| `APPLE_PASSWORD` | その Apple ID の App 用パスワード（通常のパスワードではない） |
| `APPLE_TEAM_ID` | Team ID |

- 証明書だけあって `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID` が欠けている場合は、署名のみで公証はしない。
- 公証を有効にすると、ビルドの最初に `notarytool` で資格情報を確かめる。Apple の規約更新が未承認だと数秒で失敗し、承認先（developer.apple.com/account、Account Holder のみ）を表示する。承認後に再実行する。
- 公証はビルドごとに約 10 分かかる。
- 資格情報を設定しても、Cask の隔離属性を外す処理は残る（害は無い）。不要になったら `Casks/nootle.rb` の `postflight` を消す。

この構成では、ad-hoc 署名で Apple Silicon 上のアプリが起動することと、`APPLE_SIGNING_IDENTITY=-` を Tauri が受け付けることを前提にしている。最初のリリースで、署名の状態を `codesign -dv --verbose=4 /Applications/Nootle.app` で確かめること。

### 更新用の署名鍵（Apple とは別）

アプリ内の自動更新は Tauri の署名鍵で検証する。`TAURI_SIGNING_PRIVATE_KEY` が無いとビルドが最初に失敗する。upstream の秘密鍵は持っていないので、fork は専用の鍵を持ち、`tauri.conf.json` の公開鍵も fork のものにする。そのため、upstream 版のアプリから fork 版へはアプリ内更新できない（fork 版を入れ直す）。

## upstream の変更を取り込む

差分を小さく保つため、fork 固有の変更は下の一覧に収める。upstream の変更は、ブランチを切ってマージし、PR 経由で main に入れる（main への push がリリースになるため）。

```bash
git switch -c sync/upstream-$(date +%Y%m%d) main
scripts/fork/sync-upstream.sh
git push -u origin HEAD     # PR を作り、マージコミットのまま（squash しない）main に入れる
```

スクリプトは次を行う。

- `upstream` remote が無ければ追加し、`git fetch upstream --tags` する
- `merge.ours.driver` を設定する。`.gitattributes` が `Casks/nootle.rb` と `CHANGELOG.md` に `merge=ours` を指定しているので、この 2 つは常に fork 側を残す（どちらも upstream と fork のワークフローが毎リリースで書き換えるため、放っておくと毎回衝突する）
- `upstream/main` を `--no-ff` でマージする
- マージ後に `check-updater-config.mjs` を実行し、updater の endpoint と公開鍵が fork のままかを確かめる

PR は squash せずマージする。squash すると upstream のコミットが 1 つになり、リリースノート（git-cliff）とバージョン判定が崩れる。

### fork 固有の変更の一覧（衝突しうる場所）

| ファイル | fork の変更 | 衝突したとき |
| --- | --- | --- |
| `.github/workflows/publish.yml` | Apple 資格情報の任意化、SHA256 の生成、updater のガード | upstream の変更を取り込み、`HAS_SIGNING_CERT` / `HAS_NOTARIZATION` による条件、`Configure macOS signing`、`Generate DMG checksum`、`Check updater signing config` の各ステップを残す |
| `src-tauri/tauri.conf.json` | `plugins.updater.endpoints` と `pubkey` | この 2 つは必ず fork の値を残す。取り違えるとガードが失敗する |
| `Casks/nootle.rb` | URL・homepage・`postflight` | 常に fork 側（`merge=ours`） |
| `cliff.toml` | コミット URL を `GITHUB_REPOSITORY` から作る（1 行） | upstream の変更を取り込み、この 1 行だけ再適用 |
| `README.md` | Install 節とバッジ | Install 節は fork のものを残す |
| `CHANGELOG.md` | なし（fork のリリースが再生成する） | 常に fork 側（`merge=ours`） |

次のファイルは fork 専用で、upstream には無いので衝突しない。

- `docs/fork.md`
- `scripts/fork/*`
- `.gitattributes`

`site/` や `FUNDING.yml` など、upstream の宣伝・運営用の参照は触っていない（fork では使わない）。`src-tauri/src/lib.rs` の「問題を報告」の URL も upstream の issues のままにしてある。fork では issues が無効なので、fork を指すと 404 になるため。fork で issues を有効にしたら、その 1 行を fork の URL に変える。

### マージ後の確認

```bash
git diff origin/main --stat       # 想定外のファイルが無いか
node scripts/fork/check-updater-config.mjs
```

## Whisper 対応と配布設定の分離

Whisper 対応は機能の変更、配布設定は fork の運用の変更で、変更の理由もタイミングも違う。混ぜると、upstream を取り込むときに「どの変更を残すか」が判断できなくなる。

- 配布設定は上の一覧のファイルに限り、コミットの種別は `ci:` / `build:` / `docs:` にする。機能のコミット（`feat:` / `fix:`）には配布設定を含めない
- Whisper は `feat/whisper-*` ブランチで作り、PR で main に入れる。配布系のファイルは触らない
- 同じファイルを両方が触る場合（`publish.yml` に Whisper 用の追加が要る、など）は、別コミットにする
- upstream が同等の機能を取り込んだら、fork 側の実装を捨てて upstream のものに寄せる

## 確認できていないこと

この変更を入れた時点では、次を実機で確かめていない。最初のリリースで確認する。

- `APPLE_SIGNING_IDENTITY=-` による ad-hoc 署名のビルドが通ること
- ad-hoc 署名のアプリが、実際に TCC の許可を保てるか（更新ごとに外れるか）
- mise が `Casks/nootle.rb` の `postflight` を実行できること（mise 自身の cask shim で、`postflight` が `/usr/bin/xattr` の起動まで進むことだけ Linux 上で確かめた）
- fork に push したときの Actions の権限（main への push が保護されていると、リリース後の Cask 更新が失敗する）
