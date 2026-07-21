#!/bin/bash
# BSProxy Manager - Free
PROXY_BIN="/opt/bsproxy/proxy"
SERVICE_PREFIX="bsproxy-"
DEFAULT_TARGET="127.0.0.1:22"
DEFAULT_STATUS="SSHPRO"

list_ports() {
    systemctl list-units --type=service --all --no-legend "${SERVICE_PREFIX}*.service" 2>/dev/null \
        | awk '{print $1}' \
        | sed -E "s/^${SERVICE_PREFIX}([0-9]+)\.service\$/\1/"
}

show_menu() {
    clear
    local ports
    ports=$(list_ports | tr '\n' ' ')
    [ -z "$ports" ] && ports="nenhuma"

    # Banner BSPROXY
    echo -e "\033[0;34m   ██████╗ ███████╗██████╗ ██████╗  ██████╗ ██╗  ██╗██╗   ██╗"
    echo -e "\033[0;37m   ██╔══██╗██╔════╝██╔══██╗██╔══██╗██╔═══██╗╚██╗██╔╝╚██╗ ██╔╝"
    echo -e "\033[0;34m   ██████╔╝███████╗██████╔╝██████╔╝██║   ██║ ╚███╔╝  ╚████╔╝ "
    echo -e "\033[0;37m   ██╔══██╗╚════██║██╔═══╝ ██╔══██╗██║   ██║ ██╔██╗   ╚██╔╝  "
    echo -e "\033[0;34m   ██████╔╝███████║██║     ██║  ██║╚██████╔╝██╔╝ ██╗   ██║   "
    echo -e "\033[0;37m   ╚═════╝ ╚══════╝╚═╝     ╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝   ╚═╝   "
    echo -e "\033[0;34m--------------------------------------------------------------\033[0m"

    echo "================= @BSManager ================="
    echo "|                 BSPROXY                      |"
    echo "------------------------------------------------"
    echo "| Porta(s): $ports"
    echo "------------------------------------------------"
    echo "| 1 - Abrir Porta"
    echo "| 2 - Fechar Porta"
    echo "| 0 - Sair"
    echo "------------------------------------------------"
}

open_port() {
    read -rp "Digite a porta que deseja abrir: " port
    if ! [[ "$port" =~ ^[0-9]+$ ]] || [ "$port" -lt 1 ] || [ "$port" -gt 65535 ]; then
        echo "Porta inválida."
        sleep 2
        return
    fi

    local service="${SERVICE_PREFIX}${port}.service"
    if [ -f "/etc/systemd/system/${service}" ]; then
        echo "Essa porta já está aberta pelo BSProxy."
        sleep 2
        return
    fi

    cat > "/etc/systemd/system/${service}" <<EOF
[Unit]
Description=BSProxy na porta ${port}
After=network.target

[Service]
Type=simple
ExecStart=${PROXY_BIN} --port ${port} --status "${DEFAULT_STATUS}" --target ${DEFAULT_TARGET}
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    systemctl enable "${service}" > /dev/null 2>&1
    systemctl start "${service}"

    sleep 1
    if systemctl is-active --quiet "${service}"; then
        echo "Porta ${port} aberta com sucesso."
    else
        echo "Falha ao iniciar. Veja: journalctl -u ${service} --no-pager"
        rm -f "/etc/systemd/system/${service}"
        systemctl daemon-reload
    fi
    sleep 2
}

close_port() {
    local ports
    ports=$(list_ports)
    if [ -z "$ports" ]; then
        echo "Nenhuma porta aberta no momento."
        sleep 2
        return
    fi

    echo "Portas abertas: $(echo "$ports" | tr '\n' ' ')"
    read -rp "Digite a porta que deseja fechar: " port
    local service="${SERVICE_PREFIX}${port}.service"

    if [ ! -f "/etc/systemd/system/${service}" ]; then
        echo "Essa porta não está aberta pelo BSProxy."
        sleep 2
        return
    fi

    systemctl stop "${service}"
    systemctl disable "${service}" > /dev/null 2>&1
    rm -f "/etc/systemd/system/${service}"
    systemctl daemon-reload

    echo "Porta ${port} fechada com sucesso."
    sleep 2
}

while true; do
    show_menu
    read -rp "--> Selecione uma opção: " opt
    case "$opt" in
        1) open_port ;;
        2) close_port ;;
        0) exit 0 ;;
        *) echo "Opção inválida."; sleep 1 ;;
    esac
done
