# GitHub CLI Setup

## Install `gh`

Download the latest binary to `~/bin`:

```bash
mkdir -p ~/bin
ARCH=$(uname -m)
[ "$ARCH" = "x86_64" ] && ARCH="amd64"
[ "$ARCH" = "aarch64" ] && ARCH="arm64"
GH_VERSION=$(curl -sL https://api.github.com/repos/cli/cli/releases/latest | grep '"tag_name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/')
curl -sL "https://github.com/cli/cli/releases/download/v${GH_VERSION}/gh_${GH_VERSION}_linux_${ARCH}.tar.gz" | tar -xz -C /tmp
cp /tmp/gh_${GH_VERSION}_linux_${ARCH}/bin/gh ~/bin/gh
chmod +x ~/bin/gh
rm -rf /tmp/gh_*
```

## Add `~/bin` to PATH

Add to `~/.bashrc`:

```bash
export PATH="$HOME/bin:$PATH"
```

Then reload:

```bash
source ~/.bashrc
```

## Log in with OAuth (device flow)

```bash
gh auth login --hostname github.com -p https --git-protocol https
```

This prints a one-time code and a URL. Open the URL in a browser, enter the code, and authorize.

The OAuth token is cached at `~/.config/gh/hosts.yml` automatically.

## Verify

```bash
gh auth status
```
