# OBS Chzzk Extension
[NAVER Chzzk](https://chzzk.naver.com) の OBS Studio 拡張機能

[English](README.md) | [한국어](README.ko.md) | [繁體中文](README.zh-TW.md)

## プレビュー
![プレビュー](public/image/preview-all.png)

## 前提条件
- (推奨 / 動作確認済み) OBS Studio - 32.1.1 以上
- (推奨 / 動作確認済み) Rust コンパイラ - 1.94.1
- (推奨) Linux - Debian 13 / Ubuntu 24.04 LTS / Fedora 43 / その他の新しいディストリビューション
- `pkg-config` で検出できる Qt 6 開発パッケージ
  - Debian / Ubuntu: `qt6-base-dev`
  - Fedora: `qt6-qtbase-devel`

## ビルドに関する注意
- このプロジェクトは Qt 6 のみを対象にビルドします。
- 実行時も OBS Studio 本体とこのプラグインで同じ Qt メジャーバージョンを使用する必要があります。

## インストール
```bash
# リポジトリをクローン
git clone https://github.com/zeroday0619/obs-chzzk-extension.git
cd obs-chzzk-extension

# OBS Chzzk Extension をビルド
make release

# OBS Chzzk Extension をインストール
sudo cp target/release/libobs_chzzk_extension.so /usr/lib/x86_64-linux-gnu/obs-plugins/
```

## Debian パッケージング
- Debian パッケージング用ファイルは `debian/` ディレクトリにあります。
- パッケージング補助スクリプトは `scripts/` ディレクトリにあります。
- Debian パッケージング用コンテナは `docker/debian-package/` ディレクトリにあります。
- GitHub Actions ワークフロー `.github/workflows/build-and-package.yml` は、push、pull request、タグ、手動実行でリリースプラグインと Debian パッケージをビルドします。
- ワークフローは `libobs_chzzk_extension.so` と Debian パッケージ成果物をワークフローアーティファクトとしてアップロードします。

```bash
# git コミットログから debian/changelog を生成
scripts/generate-debian-changelog.sh --since-ref <git-tag-or-commit>

# Debian パッケージをビルド
scripts/build-deb.sh --generate-changelog --since-ref <git-tag-or-commit> --clean

# Debian パッケージをビルドして ./dist に集約
scripts/build-deb.sh --generate-changelog --since-ref <git-tag-or-commit> --clean --artifacts-dir dist
```

- `scripts/generate-debian-changelog.sh` は Git のコミット subject を読み取り、Debian 形式の changelog エントリを生成します。
- `scripts/build-deb.sh` は `dpkg-buildpackage` をラップし、ビルド前に changelog を再生成できます。
- 生成された `.deb`、`.buildinfo`、`.changes` などのパッケージ成果物は、デフォルトではリポジトリの親ディレクトリに出力されます。
- Docker のように親ディレクトリが永続化されない環境では、`--artifacts-dir <dir>` オプションでリポジトリ内ディレクトリへ再コピーできます。

### Docker でビルド
```bash
# パッケージングイメージをビルド
docker build -f docker/debian-package/Dockerfile -t obs-chzzk-extension-deb .

# コンテナ内で Debian パッケージをビルド
docker run --rm \
  -v "$PWD":/work \
  -w /work \
  obs-chzzk-extension-deb \
  scripts/build-deb.sh --clean --artifacts-dir dist

# 例: ビルド前に changelog を再生成
docker run --rm \
  -v "$PWD":/work \
  -w /work \
  obs-chzzk-extension-deb \
  scripts/build-deb.sh --generate-changelog --since-ref <git-tag-or-commit> --clean --artifacts-dir dist
```

- コンテナには Debian パッケージングツール、Rust、Qt 6 のビルド依存関係が含まれます。
- コンテナには、このプロジェクトの推奨バージョンと同じ Rust `1.94.1` が `rustup` でインストールされます。
- `--artifacts-dir dist` を併用すると、コンテナ終了後もホスト側の `./dist` ディレクトリでパッケージ成果物を確認できます。

## 使い方
1. OBS Studio を起動します。
2. `Tools` > `OBS Chzzk Extension Settings` に移動します。
    - Chzzk Client ID と Secret の発行方法
        1. [Chzzk Developers](https://developers.chzzk.naver.com) > `내 서비스` > `애플리케이션 등록`
        2. `애플리케이션 ID` と `애플리케이션 이름` を設定します。
        3. `Redirect URI` を `http://127.0.0.1:20132/callback` に設定します。
        4. `API Scope` を `채널 정보 조회`, `채널 관리자 조회`, `채팅 메시지 조회`, `채팅 메시지 쓰기`, `채팅 공지 쓰기`, `채팅 설정 조회`, `채팅 설정 변경`, `후원 조회`, `방송 설정 조회`, `방송 설정 변경`, `활동제한 조회`, `활동제한 쓰기`, `구독 조회`, `유저 조회` に設定します。
        5. `등록` をクリックしてアプリケーションを作成し、`Client ID` と `Client Secret` を取得します。
    - Discord Application ID の作成方法
        1. [Discord Developer Portal](https://discord.com/developers/applications) > `New Application`
        2. `Name` を設定して `Create` をクリックします。
        3. `Rich Presence` > `Enable Rich Presence` に移動します。
        4. `Save Changes` をクリックして設定を適用し、`Application ID` を取得します。
3. 必要に応じて設定を行います。
4. `Ok` をクリックして設定を適用します。
5. `Docks` > `CHZZK Live Editor` で配信タイトル/カテゴリ/タグを編集します。

## 機能
### 実装済み
- Discord Rich Presence 連携
- ライブ配信情報の編集（タイトル / カテゴリ / タグ / サムネイルプレビュー / 並び替え）

## コントリビューション
コントリビューションを歓迎します！

改善やバグ修正のための pull request や issue をぜひ送ってください。

### なぜ OBS Chzzk Extension は Linux のみ対応ですか？
特別な理由はありません。私が主に Linux ディストリビューションを使っているためです。このプロジェクトはすべてのプラットフォームで動作することを目指しており、それがこのプロジェクトの方向性です。したがって、クロスプラットフォーム対応に関するコントリビューションが提出された場合は、非常に前向きにレビューし、承認したいと考えています。

## ライセンス
このプロジェクトは MIT ライセンスのもとで提供されています。詳細は [LICENSE](LICENSE) ファイルを参照してください。

## AI生成コードに関する告知

このプロジェクトの一部は、AIツール（大規模言語モデルなど）の支援を受けて作成されました。すべてのAI支援による貢献は、含める前にメンテナーによってレビューおよび調整されています。特定の変更の出所が必要な場合は、Gitの履歴とコミットメッセージを参照してください。
