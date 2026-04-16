# OBS Chzzk Extension
[NAVER Chzzk](https://chzzk.naver.com) 的 OBS Studio 擴充套件

[English](README.md) | [한국어](README.ko.md) | [日本語](README.ja.md)

## 預覽
![預覽](public/image/preview-all.png)

## 事前需求
- （建議 / 已測試）OBS Studio - 32.1.1 以上
- （建議 / 已測試）Rust 編譯器 - 1.94.1
- （建議）Linux - Debian 13 / Ubuntu 24.04 LTS / Fedora 43 / 其他較新的發行版
- 可透過 `pkg-config` 偵測到的 Qt 6 開發套件
  - Debian / Ubuntu: `qt6-base-dev`
  - Fedora: `qt6-qtbase-devel`

## 建置說明
- 此專案現在僅針對 Qt 6 建置。
- 執行時的 OBS Studio 與此外掛也必須使用相同的 Qt 主版本。

## 安裝
```bash
# 複製儲存庫
git clone https://github.com/zeroday0619/obs-chzzk-extension.git
cd obs-chzzk-extension

# 建置 OBS Chzzk Extension
make release

# 安裝 OBS Chzzk Extension
sudo cp target/release/libobs_chzzk_extension.so /usr/lib/x86_64-linux-gnu/obs-plugins/
```

## Debian 封裝
- Debian 封裝相關檔案位於 `debian/` 目錄。
- 封裝輔助腳本位於 `scripts/` 目錄。
- Debian 封裝用容器位於 `docker/debian-package/` 目錄。
- GitHub Actions 工作流程 `.github/workflows/build-and-package.yml` 會在 push、pull request、標籤與手動執行時建置 release 外掛與 Debian 套件。
- 工作流程會將 `libobs_chzzk_extension.so` 與 Debian 封裝產物上傳為 workflow artifacts。

```bash
# 根據 git commit 記錄產生 debian/changelog
scripts/generate-debian-changelog.sh --since-ref <git-tag-or-commit>

# 建置 Debian 套件
scripts/build-deb.sh --generate-changelog --since-ref <git-tag-or-commit> --clean

# 建置 Debian 套件並集中到 ./dist
scripts/build-deb.sh --generate-changelog --since-ref <git-tag-or-commit> --clean --artifacts-dir dist
```

- `scripts/generate-debian-changelog.sh` 會讀取 Git commit subject，並產生 Debian 格式的 changelog 項目。
- `scripts/build-deb.sh` 封裝了 `dpkg-buildpackage`，並可在建置前重新產生 changelog。
- 預設情況下，產生的 `.deb`、`.buildinfo`、`.changes` 等套件產物會寫到儲存庫的上一層目錄。
- 在像 Docker 這類上一層目錄可能不具持久性的環境中，可以使用 `--artifacts-dir <dir>` 選項將產物再複製回儲存庫內的目錄。

### 在 Docker 中建置
```bash
# 建立封裝映像
docker build -f docker/debian-package/Dockerfile -t obs-chzzk-extension-deb .

# 在容器內建置 Debian 套件
docker run --rm \
  -v "$PWD":/work \
  -w /work \
  obs-chzzk-extension-deb \
  scripts/build-deb.sh --clean --artifacts-dir dist

# 範例：建置前重新產生 changelog
docker run --rm \
  -v "$PWD":/work \
  -w /work \
  obs-chzzk-extension-deb \
  scripts/build-deb.sh --generate-changelog --since-ref <git-tag-or-commit> --clean --artifacts-dir dist
```

- 容器中包含 Debian 封裝工具鏈、Rust 與 Qt 6 建置依賴。
- 容器會透過 `rustup` 安裝 Rust `1.94.1`，與本專案建議使用的工具鏈一致。
- 搭配 `--artifacts-dir dist` 使用時，容器結束後仍可在主機端的 `./dist` 目錄中看到套件產物。

## 使用方式
1. 開啟 OBS Studio。
2. 前往 `Tools` > `OBS Chzzk Extension Settings`。
    - 如何取得 Chzzk Client ID 與 Secret
        1. [Chzzk Developers](https://developers.chzzk.naver.com) > `내 서비스` > `애플리케이션 등록`
        2. 設定 `애플리케이션 ID` 與 `애플리케이션 이름`
        3. 將 `Redirect URI` 設定為 `http://127.0.0.1:20132/callback`
        4. 將 `API Scope` 設定為 `채널 정보 조회`、`채널 관리자 조회`、`채팅 메시지 조회`、`채팅 메시지 쓰기`、`채팅 공지 쓰기`、`채팅 설정 조회`、`채팅 설정 변경`、`후원 조회`、`방송 설정 조회`、`방송 설정 변경`、`활동제한 조회`、`활동제한 쓰기`、`구독 조회`、`유저 조회`
        5. 點擊 `등록` 建立應用程式並取得 `Client ID` 與 `Client Secret`
    - 如何建立 Discord Application ID
        1. [Discord Developer Portal](https://discord.com/developers/applications) > `New Application`
        2. 設定 `Name`，然後點擊 `Create`
        3. 前往 `Rich Presence` > `Enable Rich Presence`
        4. 點擊 `Save Changes` 套用設定並取得 `Application ID`
3. 依需求調整設定。
4. 點擊 `Ok` 套用設定。
5. 前往 `Docks` > `CHZZK Live Editor` 編輯直播標題 / 分類 / 標籤。

## 功能
### 已實作
- Discord Rich Presence 整合
- 編輯直播資訊（標題 / 分類 / 標籤 / 縮圖預覽 / 排序）

## 貢獻
歡迎貢獻！

如果有任何改進建議或錯誤修正，歡迎提交 pull request 或開 issue。

### 為什麼 OBS Chzzk Extension 只支援 Linux？
沒有什麼特別的原因，只是因為我主要使用 Linux 發行版。我希望這個專案最終能在所有平台上運作，這也是這個專案的方向。因此，如果有人提交與跨平台支援相關的貢獻，我會非常樂於審查並積極接受。

## 授權條款
本專案採用 MIT 授權條款，詳情請參閱 [LICENSE](LICENSE) 檔案。

## AI 生成程式碼說明

本專案的部分內容是在 AI 工具（例如大型語言模型）的協助下建立的。所有 AI 協助產生的貢獻在納入前都已由維護者審查與調整。如需追溯特定變更的來源，請參閱 Git 歷史與 commit 訊息。
