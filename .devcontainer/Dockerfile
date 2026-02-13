FROM mcr.microsoft.com/devcontainers/base:ubuntu

ARG TZ
ENV TZ="$TZ"

# Install development tools and iptables/ipset
RUN apt-get update && apt-get install -y --no-install-recommends \
    less \
    procps \
    fzf \
    man-db \
    unzip \
    gnupg2 \
    iptables \
    ipset \
    iproute2 \
    dnsutils \
    aggregate \
    jq \
    nano \
    vim \
    fish

# Set `DEVCONTAINER` environment variable to help with orientation
ENV DEVCONTAINER=true

# Create workspace directory and set permissions
RUN mkdir -p /workspace && \
    chown -R vscode:vscode /workspace

WORKDIR /workspace

# Switch to non-root user
USER vscode

# Set the default shell to fish
ENV SHELL=/usr/bin/fish

# Set the default editor and visual
ENV EDITOR=nano
ENV VISUAL=nano

RUN curl -sS https://starship.rs/install.sh | sh -s -- --yes \
    && mkdir -p /home/vscode/.config/fish \
    && echo 'fish_add_path /home/vscode/.local/bin' >> /home/vscode/.config/fish/config.fish \
    && echo 'alias l="ls -lisa"' >> /home/vscode/.config/fish/config.fish \
    && echo 'alias claude="claude --dangerously-skip-permissions"' >> /home/vscode/.config/fish/config.fish \
    && echo 'starship init fish | source' >> /home/vscode/.config/fish/config.fish \
    && curl -fsSL https://claude.ai/install.sh | bash

# Copy firewall script (allowed-domains.conf is bind-mounted at
# runtime so changes take effect on container restart without rebuild)
COPY init-firewall.sh /usr/local/bin/
USER root
RUN chmod +x /usr/local/bin/init-firewall.sh && \
    echo "vscode ALL=(root) NOPASSWD: /usr/local/bin/init-firewall.sh" > /etc/sudoers.d/vscode-firewall && \
    chmod 0440 /etc/sudoers.d/vscode-firewall
USER vscode
