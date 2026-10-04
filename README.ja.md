# Angelic Angel

> 「Angelic Angel/Hello,星を数えて」は、2015年7月1日に Lantis から発売された μ's によるシングルで、楽曲は劇場版『ラブライブ！The School Idol Movie』の挿入歌。
>
> — [Wikipedia](https://ja.wikipedia.org/wiki/Angelic_Angel/Hello,%E6%98%9F%E3%82%92%E6%95%B0%E3%81%88%E3%81%A6)

Twitter/X の通知を Mozilla の Web Push 基盤を通じてリアルタイムに受信する CLI ツールです。自分がフォローしていて、ツイート通知を有効化しているユーザのツイートをストリーミングできます。

[English README](README.md)

## 概要

Angelic Angel はブラウザの Web Push クライアントをエミュレートし、Twitter/X のプッシュ通知を受信します。[Mozilla AutoPush](https://autopush.readthedocs.io/) に WebSocket で接続し、ECE (Encrypted Content-Encoding) で暗号化された通知を復号して、設定された Webhook エンドポイントに転送します。

フォロー中かつ **ツイート通知をオン** にしているユーザのツイートがリアルタイムで届きます。

### 仕組み

```
Twitter/X  ──push──▶  Mozilla AutoPush サーバ  ◀──WebSocket──  Angelic Angel  ──HTTP POST──▶  Webhook
```

1. Angelic Angel が Mozilla AutoPush サーバに Web Push サブスクライバとして登録します。
2. 取得したプッシュサブスクリプションのエンドポイントを Twitter の通知設定 API に登録します。
3. Twitter がプッシュ通知を送信すると、Firefox が使用するものと同じ Mozilla AutoPush サーバを経由して届きます。
4. Angelic Angel が WebSocket 経由で通知を受信・復号し、Webhook にペイロードを転送します。

### 重要事項

- **データの取得元**: すべての通知データは Mozilla の Web Push サーバ (`push.services.mozilla.com`) から受信しています。通知データの取得のために Twitter/X に直接アクセスすることはありません。
- **API の使用は最小限**: Twitter/X の API はプッシュサブスクリプションの初回登録時 (`register` コマンド) にのみ使用されます。通知の受信中に API コールは発生しません。
- **スクレイピング不使用**: このツールは Web スクレイピングを一切行いません。ブラウザがプッシュ通知を配信するのと同じ、標準的な W3C Push API のフローを利用しています。

## 必要環境

- Rust (edition 2024)
- Twitter/X アカウントの認証情報 (`auth_token` と `ct0` Cookie)

### `auth_token` と `ct0` の取得方法

1. Web ブラウザで [x.com](https://x.com) を開いてログインします。
2. 開発者ツール (F12) を開き、**Application** (または **ストレージ**) タブを選択します。
3. **Cookie** → `https://x.com` から `auth_token` と `ct0` の値を確認できます。

## インストール

```sh
cargo install --path .
```

## 使い方

### 1. 設定の初期化

```sh
# 対話モード
angelic-angel init

# 引数を指定する場合
angelic-angel init --auth-token YOUR_AUTH_TOKEN --ct0 YOUR_CT0

# systemd credentials を使い、Cookie を設定ファイルに保存しない場合
angelic-angel init --systemd-credentials
```

通常モードでは Twitter/X の Cookie を `angelic-angel.toml` に保存します。`--systemd-credentials` を指定すると、設定ファイルには `auth_token` / `ct0` を保存せず、実行時に systemd credential の `auth_token` と `ct0` を読み込みます。

### systemd credentials を使う場合

常駐サーバでは、X のログイン Cookie を設定ファイルに保存せず systemd credentials で渡せます。

```ini
[Service]
User=angelic-angel
LoadCredential=auth_token:/run/angelic-angel-secrets/auth_token
LoadCredential=ct0:/run/angelic-angel-secrets/ct0
```

systemd は credential をサービス専用ディレクトリにコピーし、その場所を `$CREDENTIALS_DIRECTORY` としてプロセスへ渡します。Angelic Angel は `$CREDENTIALS_DIRECTORY/auth_token` と `$CREDENTIALS_DIRECTORY/ct0` を読み、TOML 内の値より優先します。片方だけ存在する場合は、別の保存元と混在させずエラー終了します。

Cookie を含まない永続設定は次のように作成します。

```sh
sudo install -d -o angelic-angel -g angelic-angel -m 0700 /var/lib/angelic-angel
sudo -u angelic-angel angelic-angel \
  -c /var/lib/angelic-angel/angelic-angel.toml init --systemd-credentials
```

最初の `register` も credential が読み込まれた状態で実行する必要があります。たとえばシークレットローダーが root のみ読み取り可能なファイルを作成した後:

```sh
sudo systemd-run --wait --pipe \
  -p User=angelic-angel \
  -p Group=angelic-angel \
  -p LoadCredential=auth_token:/run/angelic-angel-secrets/auth_token \
  -p LoadCredential=ct0:/run/angelic-angel-secrets/ct0 \
  /usr/local/bin/angelic-angel \
  -c /var/lib/angelic-angel/angelic-angel.toml register
```

登録後は通常の systemd サービスとして常駐させます。例は [`examples/angelic-angel.service`](examples/angelic-angel.service) を参照してください。

設定ファイルには Web Push の秘密鍵と登録状態が残るため、Unix では保存時に mode `0600` を設定します。systemd credential モードでは X の Cookie 自体は設定ファイルへ書き込みません。

### 2. プッシュサブスクリプションの登録

```sh
angelic-angel register
```

Mozilla AutoPush に新しいプッシュサブスクリプションを登録し、そのエンドポイントを Twitter のプッシュ通知 API に登録します。

### 3. 通知の受信開始

```sh
WEBHOOK_ENDPOINT=https://your-webhook.example.com/endpoint angelic-angel listen
```

`WEBHOOK_ENDPOINT` 環境変数で、復号された通知ペイロードの HTTP POST 送信先を指定します。

### その他のコマンド

```sh
# 現在の設定と登録状態を確認
angelic-angel status

# プッシュサブスクリプションを解除
angelic-angel unregister
```

### オプション

| フラグ | 説明 |
|--------|------|
| `-c, --config <PATH>` | 設定ファイルのパス (デフォルト: `angelic-angel.toml`) |
| `-v, --verbose` | デバッグログを有効化 |

## 再接続

Angelic Angel は Firefox 互換の再接続戦略を実装しています:

- 指数バックオフ: 5秒 × 2^n (上限 5 分)
- UAID 無効化時の自動再登録
- サーババックオフ (close code 4774): 30 分間の待機
- 接続成功時にリトライカウンタをリセットする無限リトライ

## ライセンス

MIT
