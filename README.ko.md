# OBS Chzzk Extension
[NAVER Chzzk](https://chzzk.naver.com) OBS Studio 확장 프로그램

[English](README.md) | [日本語](README.ja.md) | [繁體中文](README.zh-TW.md)

## 미리보기
![미리보기](public/image/preview-all.png)

## 사전 요구 사항
- (권장 / 테스트 완료) OBS Studio - 32.1.1 이상
- (권장 / 테스트 완료) Rust 컴파일러 - 1.94.1
- (권장) Linux - Debian 13 / Ubuntu 24.04 LTS / Fedora 43 / 기타 최신 배포판
- `pkg-config`로 찾을 수 있는 Qt 6 개발 패키지
  - Debian / Ubuntu: `qt6-base-dev`
  - Fedora: `qt6-qtbase-devel`

## 빌드 참고
- 이 프로젝트는 이제 Qt 6만 대상으로 빌드합니다.
- 런타임에서도 OBS Studio와 이 플러그인이 같은 Qt 메이저 버전을 사용해야 합니다.

## 설치
```bash
# 저장소 클론
git clone https://github.com/zeroday0619/obs-chzzk-extension.git
cd obs-chzzk-extension

# OBS Chzzk Extension 빌드
make release

# OBS Chzzk Extension 설치
sudo cp target/release/libobs_chzzk_extension.so /usr/lib/x86_64-linux-gnu/obs-plugins/
```

## 데비안 패키징
- 데비안 패키징 파일은 `debian/` 디렉터리에 있습니다.
- 패키징 보조 스크립트는 `scripts/` 디렉터리에 있습니다.
- 데비안 패키징용 컨테이너는 `docker/debian-package/` 디렉터리에 있습니다.

```bash
# git 커밋 로그 기반으로 debian/changelog 생성
scripts/generate-debian-changelog.sh --since-ref <git-tag-or-commit>

# 데비안 패키지 빌드
scripts/build-deb.sh --generate-changelog --since-ref <git-tag-or-commit> --clean

# 데비안 패키지를 빌드하고 ./dist 에 모으기
scripts/build-deb.sh --generate-changelog --since-ref <git-tag-or-commit> --clean --artifacts-dir dist
```

- `scripts/generate-debian-changelog.sh` 는 Git 커밋 subject를 읽어 Debian 형식의 changelog 항목을 생성합니다.
- `scripts/build-deb.sh` 는 `dpkg-buildpackage` 를 감싸며, 빌드 전에 changelog를 다시 생성할 수 있습니다.
- 생성된 `.deb`, `.buildinfo`, `.changes` 같은 패키지 산출물은 기본적으로 저장소의 상위 디렉터리에 기록됩니다.
- Docker처럼 상위 디렉터리가 휘발성일 수 있는 환경에서는 `--artifacts-dir <dir>` 옵션으로 저장소 안 디렉터리에 다시 복사할 수 있습니다.

### Docker에서 빌드
```bash
# 패키징 이미지 빌드
docker build -f docker/debian-package/Dockerfile -t obs-chzzk-extension-deb .

# 컨테이너 안에서 데비안 패키지 빌드
docker run --rm \
  -v "$PWD":/work \
  -w /work \
  obs-chzzk-extension-deb \
  scripts/build-deb.sh --clean --artifacts-dir dist

# 예시: 빌드 전에 changelog 다시 생성
docker run --rm \
  -v "$PWD":/work \
  -w /work \
  obs-chzzk-extension-deb \
  scripts/build-deb.sh --generate-changelog --since-ref <git-tag-or-commit> --clean --artifacts-dir dist
```

- 컨테이너에는 Debian 패키징 도구, Rust, Qt 6 빌드 의존성이 포함됩니다.
- 컨테이너에는 이 프로젝트 권장 버전과 같은 Rust `1.94.1`이 `rustup`으로 설치됩니다.
- `--artifacts-dir dist`를 함께 쓰면 컨테이너 종료 후에도 호스트의 `./dist` 디렉터리에서 패키지 산출물을 확인할 수 있습니다.

## 사용 방법
1. OBS Studio를 실행합니다.
2. `Tools` > `OBS Chzzk Extension Settings`로 이동합니다.
    - Chzzk Client ID 및 Secret 생성 방법
        1. [Chzzk Developers](https://developers.chzzk.naver.com) > `내 서비스` > `애플리케이션 등록`
        2. `애플리케이션 ID`와 `애플리케이션 이름`을 설정합니다.
        3. `Redirect URI`를 `http://127.0.0.1:20132/callback`으로 설정합니다.
        4. `API Scope`를 `채널 정보 조회`, `채널 관리자 조회`, `채팅 메시지 조회`, `채팅 메시지 쓰기`, `채팅 공지 쓰기`, `채팅 설정 조회`, `채팅 설정 변경`, `후원 조회`, `방송 설정 조회`, `방송 설정 변경`, `활동제한 조회`, `활동제한 쓰기`, `구독 조회`, `유저 조회`로 설정합니다.
        5. `등록`을 클릭해 애플리케이션을 생성하고 `Client ID`와 `Client Secret`을 발급받습니다.
    - Discord Application ID 생성 방법
        1. [Discord Developer Portal](https://discord.com/developers/applications) > `New Application`
        2. `Name`을 설정하고 `Create`를 클릭합니다.
        3. `Rich Presence` > `Enable Rich Presence`로 이동합니다.
        4. `Save Changes`를 클릭해 설정을 적용하고 `Application ID`를 확인합니다.
3. 필요한 설정을 구성합니다.
4. `Ok`를 클릭하여 설정을 적용합니다.
5. `Docks` > `CHZZK Live Editor`에서 방송 제목/카테고리/태그를 수정합니다.

## 기능
### 구현됨
- Discord Rich Presence 연동
- 라이브 스트림 정보 편집 (제목 / 카테고리 / 태그 / 썸네일 미리보기 / 정렬)

## 기여
기여를 환영합니다!

개선 사항이나 버그 수정을 위해 pull request 또는 issue를 자유롭게 등록해 주세요.

### 왜 OBS Chzzk Extension은 Linux만 지원하나요?
특별한 이유는 없습니다. 제가 주로 Linux 배포판을 사용하기 때문입니다. 이 프로젝트는 모든 플랫폼에서 동작하는 것을 목표로 하고 있으며, 그것이 이 프로젝트의 방향입니다. 따라서 크로스플랫폼 지원 관련 기여가 제출된다면 매우 긍정적으로 검토하고 승인할 예정입니다.

## 라이선스
이 프로젝트는 MIT 라이선스를 따르며, 자세한 내용은 [LICENSE](LICENSE) 파일을 참고하세요.

## AI 생성 코드 고지

이 프로젝트의 일부는 AI 도구(예: 대규모 언어 모델)의 도움을 받아 작성되었습니다. 모든 AI 지원 기여는 포함 전에 메인테이너가 검토하고 수정했습니다. 특정 변경 사항의 출처가 필요한 경우 Git 히스토리와 커밋 메시지를 참조하세요.
