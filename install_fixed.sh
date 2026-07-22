#!/bin/bash
# AWProxy Fixed Installer
CMD_NAME="awproxy"
TOTAL_STEPS=9
CURRENT_STEP=0

show_progress() {
    PERCENT=$((CURRENT_STEP * 100 / TOTAL_STEPS))
    echo -e "\033[1;34mProgresso: [${PERCENT}%] - $1\033[0m"
}

error_exit() {
    echo -e "\n\033[1;31mErro: $1\033[0m"
    echo -e "Verifique se você tem conexão com a internet e espaço em disco."
    exit 1
}

increment_step() {
    CURRENT_STEP=$((CURRENT_STEP + 1))
}

if [ "$EUID" -ne 0 ]; then
    error_exit "POR FAVOR, EXECUTE COMO ROOT (sudo su)"
fi

clear
# Banner AWPROXY
echo -e "\033[0;34m    █████╗ ██╗    ██╗██████╗ ██████╗  ██████╗ ██╗  ██╗██╗   ██╗"
echo -e "\033[0;37m   ██╔══██╗██║    ██║██╔══██╗██╔══██╗██╔═══██╗╚██╗██╔╝╚██╗ ██╔╝"
echo -e "\033[0;34m   ███████║██║ █╗ ██║██████╔╝██████╔╝██║   ██║ ╚███╔╝  ╚████╔╝ "
echo -e "\033[0;37m   ██╔══██║██║███╗██║██╔═══╝ ██╔══██╗██║   ██║ ██╔██╗   ╚██╔╝  "
echo -e "\033[0;34m   ██║  ██║╚███╔███╔╝██║     ██║  ██║╚██████╔╝██╔╝ ██╗   ██║   "
echo -e "\033[0;37m   ╚═╝  ╚═╝ ╚══╝╚══╝ ╚═╝     ╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝   ╚═╝   "
echo -e "\033[0;34m--------------------------------------------------------------\033[0m"

show_progress "Atualizando repositórios e instalando dependências base..."
export DEBIAN_FRONTEND=noninteractive
apt update -y || error_exit "Falha ao atualizar os repositórios. Verifique sua internet."
apt install -y curl build-essential git lsb-release libssl-dev pkg-config || error_exit "Falha ao instalar dependências essenciais (build-essential, libssl-dev, etc)."
increment_step

show_progress "Verificando compatibilidade do sistema..."
OS_NAME=$(lsb_release -is)
VERSION=$(lsb_release -rs)
if [[ "$OS_NAME" != "Ubuntu" && "$OS_NAME" != "Debian" ]]; then
    echo -e "\033[1;33mAviso: Sistema $OS_NAME detectado. O script foi testado em Ubuntu/Debian.\033[0m"
fi
increment_step

show_progress "Preparando ambiente Rust..."
if ! command -v rustc &> /dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y || error_exit "Falha ao baixar/instalar o Rust (AWPro)."
    source "$HOME/.cargo/env"
else
    source "$HOME/.cargo/env" 2>/dev/null || true
fi
# Garantir que cargo está no PATH
export PATH="$HOME/.cargo/bin:$PATH"
increment_step

show_progress "Preparando diretórios..."
mkdir -p /opt/awproxy
INSTALL_DIR=$(pwd)
increment_step

show_progress "Compilando AWProxy (isso pode demorar alguns minutos)..."
# Tentar compilar no diretório atual (onde o zip foi extraído)
if [ -f "Cargo.toml" ]; then
    cargo build --release || {
        echo -e "\033[1;31mFalha na compilação direta. Tentando limpar cache...\033[0m"
        cargo clean
        cargo build --release || error_exit "Falha crítica na compilação do Rust. Veja os erros acima."
    }
else
    error_exit "Arquivo Cargo.toml não encontrado no diretório atual!"
fi
increment_step

show_progress "Instalando binários..."
if [ -f "./target/release/awproxy" ]; then
    cp ./target/release/awproxy /opt/awproxy/proxy
    chmod +x /opt/awproxy/proxy
else
    error_exit "Binário compilado não encontrado em ./target/release/awproxy"
fi

if [ -f "menu.sh" ]; then
    cp menu.sh /opt/awproxy/menu
    chmod +x /opt/awproxy/menu
    ln -sf /opt/awproxy/menu /usr/local/bin/awproxy
else
    ln -sf /opt/awproxy/proxy /usr/local/bin/awproxy
fi
increment_step

show_progress "Configurando permissões finais..."
chmod +x /usr/local/bin/awproxy
increment_step

show_progress "Finalizando..."
increment_step

echo ""
echo -e "\033[0;32m✅ AWProxy instalado com sucesso!\033[0m"
echo ""
echo "🚀 Digite 'awproxy' para abrir o menu."
echo ""
echo "📡 Nota: Se 'awproxy' não funcionar imediatamente, execute: source ~/.bashrc"
echo ""
