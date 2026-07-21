#!/bin/bash
# BSProxy Free
REPO_URL="https://github.com/PedroJbk/BSProxy.git"
REPO_BRANCH="main"
CMD_NAME="bsproxy"
TOTAL_STEPS=9
CURRENT_STEP=0

show_progress() {
    PERCENT=$((CURRENT_STEP * 100 / TOTAL_STEPS))
    echo "Progresso: [${PERCENT}%] - $1"
}

error_exit() {
    echo -e "\nErro: $1"
    exit 1
}

increment_step() {
    CURRENT_STEP=$((CURRENT_STEP + 1))
}

if [ "$EUID" -ne 0 ]; then
    error_exit "EXECUTE COMO ROOT"
else
    clear

    # Banner BSPROXY
    echo -e "\033[0;34m   ██████╗ ███████╗██████╗ ██████╗  ██████╗ ██╗  ██╗██╗   ██╗"
    echo -e "\033[0;37m   ██╔══██╗██╔════╝██╔══██╗██╔══██╗██╔═══██╗╚██╗██╔╝╚██╗ ██╔╝"
    echo -e "\033[0;34m   ██████╔╝███████╗██████╔╝██████╔╝██║   ██║ ╚███╔╝  ╚████╔╝ "
    echo -e "\033[0;37m   ██╔══██╗╚════██║██╔═══╝ ██╔══██╗██║   ██║ ██╔██╗   ╚██╔╝  "
    echo -e "\033[0;34m   ██████╔╝███████║██║     ██║  ██║╚██████╔╝██╔╝ ██╗   ██║   "
    echo -e "\033[0;37m   ╚═════╝ ╚══════╝╚═╝     ╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝   ╚═╝   "
    echo -e "\033[0;34m--------------------------------------------------------------\033[0m"

    show_progress "Atualizando repositorios..."
    export DEBIAN_FRONTEND=noninteractive
    apt update -y > /dev/null 2>&1 || error_exit "Falha ao atualizar os repositorios"
    increment_step

    show_progress "Verificando o sistema..."
    if ! command -v lsb_release &> /dev/null; then
        apt install lsb-release -y > /dev/null 2>&1 || error_exit "Falha ao instalar lsb-release"
    fi
    increment_step

    OS_NAME=$(lsb_release -is)
    VERSION=$(lsb_release -rs)
    case $OS_NAME in
        Ubuntu)
            case $VERSION in
                24.*|22.*|20.*|18.*) show_progress "Sistema Ubuntu suportado, continuando..." ;;
                *) error_exit "Versão do Ubuntu não suportada. Use 18, 20, 22 ou 24." ;;
            esac
            ;;
        Debian)
            case $VERSION in
                12*|11*|10*|9*) show_progress "Sistema Debian suportado, continuando..." ;;
                *) error_exit "Versão do Debian não suportada. Use 9, 10, 11 ou 12." ;;
            esac
            ;;
        *) error_exit "Sistema não suportado. Use Ubuntu ou Debian." ;;
    esac
    increment_step

    show_progress "Atualizando o sistema..."
    apt upgrade -y > /dev/null 2>&1 || error_exit "Falha ao atualizar o sistema"
    apt-get install curl build-essential git -y > /dev/null 2>&1 || error_exit "Falha ao instalar pacotes"
    increment_step

    show_progress "Criando diretorio /opt/bsproxy..."
    mkdir -p /opt/bsproxy > /dev/null 2>&1
    increment_step

    show_progress "Instalando Rust..."
    if ! command -v rustc &> /dev/null; then
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y > /dev/null 2>&1 || error_exit "Falha ao instalar Rust"
        source "$HOME/.cargo/env"
    fi
    increment_step

    show_progress "Compilando BSProxy, isso pode levar algum tempo..."
    if [ -d "/root/BSProxy" ]; then
        rm -rf /root/BSProxy
    fi
    git clone --branch "$REPO_BRANCH" "$REPO_URL" /root/BSProxy > /dev/null 2>&1 || error_exit "Falha ao clonar BSProxy"

    if [ -f /root/BSProxy/menu.sh ]; then
        cp /root/BSProxy/menu.sh /opt/bsproxy/menu
        chmod +x /opt/bsproxy/menu
    fi

    cd /root/BSProxy || error_exit "Diretório do BSProxy não encontrado"
    cargo build --release --jobs "$(nproc)" > /dev/null 2>&1 || error_exit "Falha ao compilar BSProxy"

    if [ -f ./target/release/bsproxy ]; then
        mv ./target/release/bsproxy /opt/bsproxy/proxy || error_exit "Binário compilado não encontrado"
        chmod +x /opt/bsproxy/proxy
    else
        error_exit "Binário 'bsproxy' não encontrado após compilação"
    fi
    increment_step

    show_progress "Configurando permissões..."
    chmod +x /opt/bsproxy/proxy
    [ -f /opt/bsproxy/menu ] && chmod +x /opt/bsproxy/menu

    if [ -f /opt/bsproxy/menu ]; then
        cp /opt/bsproxy/menu /usr/local/bin/bsproxy
    else
        cp /opt/bsproxy/proxy /usr/local/bin/bsproxy
    fi
    chmod +x /usr/local/bin/bsproxy
    increment_step

    show_progress "Limpando diretórios temporários..."
    cd /root/
    rm -rf /root/BSProxy/
    increment_step

    echo ""
    echo -e "\033[0;32m✅ Instalação concluída com sucesso!\033[0m"
    echo ""
    echo "🚀 Digite 'bsproxy' para acessar o menu."
    echo "   Ou 'bsproxy -p 80' para abrir porta 80 diretamente."
    echo ""
    echo "📡 Protocolos suportados:"
    echo "   - SOCKS5 (byte 0x05)"
    echo "   - TLS/SECURITY (byte 0x16)"
    echo "   - WebSocket (GET / ou HTTP/)"
    echo "   - SECURITY (AUTH ou SECURITY)"
    echo "   - TCP Fallback (qualquer outro)"
    echo ""
fi
