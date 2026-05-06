## Pré-requisitos

### 1. Dependências de sistema (build)

```bash
sudo apt install \
    build-essential \
    cmake \
    curl \
    git \
    libmpv-dev \
    libssl-dev \
    libwayland-dev \
    libxkbcommon-dev \
    mpv \
    pkg-config \
    wayland-protocols
```

> **`build-essential`** instala `gcc`, `make` e `libc6-dev`.
> **`libmpv-dev`** é a biblioteca de desenvolvimento do mpv usada em tempo de compilação;
> **`mpv`** é o runtime necessário para a reprodução dos vídeos.

### 2. Toolchain Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### 3. Ferramenta `just` (task runner)

`just` é instalado via Cargo (não está disponível no apt padrão do Ubuntu 24.04):

```bash
cargo install just
```

Após a instalação o binário fica em `~/.cargo/bin/just`.


## Compilar e instalar

```bash
git clone <url-do-repositório>
cd compact_launcher

# Compilar (release)
~/.cargo/bin/just build-release

# Instalar em /usr  (requer sudo)
sudo ~/.cargo/bin/just install
```

O `just install` copia para:

| Arquivo | Destino |
|---|---|
| `compact_launcher-applet` | `/usr/bin/compact_launcher-applet` |
| `compact_launcher` | `/usr/bin/compact_launcher` |
| `applet.desktop` | `/usr/share/applications/com.gitlab.gilsonfonsaca.compact_launcher.applet.desktop` |
| `app.desktop` | `/usr/share/applications/com.gitlab.gilsonfonsaca.compact_launcher.desktop` |
| ícone simbólico | `/usr/share/icons/hicolor/symbolic/apps/compact_launcher-applet-symbolic.svg` |

---

## Adicionar o applet ao dock COSMIC

1. Em Settings clique Desktop > Dock
2. Selecione **"Configure Dock applets"**.
3. Adicione **"Compact Launcher"** à lista de applets.
4. O ícone aparecerá na dock na posição selecionada.