FROM mcr.microsoft.com/devcontainers/base:ubuntu

# Install development tools
# hadolint ignore=DL3009
RUN apt-get update && apt-get install -y --no-install-recommends \
    less \
    procps \
    fzf \
    man-db \
    unzip \
    gnupg2 \
    dnsutils \
    jq \
    nano \
    vim \
    git \
    fish \
    sox \
    pulseaudio-utils \
    alsa-utils \
    libasound2-plugins

# Route ALSA through PulseAudio so applications that use ALSA directly
# don't fail when /dev/snd is unavailable (e.g., VM or remote host).
RUN printf 'pcm.default pulse\nctl.default pulse\n' > /etc/asound.conf

# Set `DEVCONTAINER` environment variable to help with orientation
ENV DEVCONTAINER=true

# Create workspace directory and set permissions
RUN mkdir -p /workspace && \
    chown -R vscode:vscode /workspace

WORKDIR /workspace

# Switch to non-root user
USER vscode

RUN curl -fsSL https://claude.ai/install.sh | bash

# Set the default shell to fish
ENV SHELL=/usr/bin/fish

# Set the default editor and visual
ENV EDITOR=nano
ENV VISUAL=nano

RUN curl -sS https://starship.rs/install.sh | sh -s -- --yes && \
    mkdir -p /home/vscode/.config/fish && \
    echo 'fish_add_path /home/vscode/.local/bin' >> /home/vscode/.config/fish/config.fish && \
    echo 'alias l="ls -lisa"' >> /home/vscode/.config/fish/config.fish && \
    echo 'alias claude="claude --dangerously-skip-permissions"' >> /home/vscode/.config/fish/config.fish && \
    echo 'starship init fish | source' >> /home/vscode/.config/fish/config.fish
