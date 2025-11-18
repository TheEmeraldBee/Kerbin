#!/bin/bash
set -e

BLUE='\033[0;34m'
CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

print_step() { echo -e "\n${CYAN}>>> $1${NC}"; }
print_info() { echo -e "${BLUE}  • $1${NC}"; }
print_success() { echo -e "${GREEN}  ✔ $1${NC}"; }
print_error() { echo -e "${RED}  ✖ ERROR: $1${NC}"; }

confirm() {
    echo ""
    read -p "$(echo -e ${YELLOW}"$1 (y/n): "${NC})" -n 1
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        print_info "Cancelled"
        exit 1
    fi
}

check_command() {
    if ! command -v "$1" &>/dev/null; then
        print_error "Command '$1' could not be found. Please install it to continue."
        exit 1
    fi
    print_success "$1 is installed."
}

KERBIN_ROOT="${HOME}/.kerbin"
KERBIN_INSTALL_CONFIG="${KERBIN_ROOT}/install_config"
KERBIN_BIN_DIR="${KERBIN_ROOT}/bin"
KERBIN_BUILD_DIR="${KERBIN_ROOT}/build"

SELECTED_VERSION=""
CONFIG_PATH=""

if [ -n "${XDG_CONFIG_HOME}" ]; then
    DEFAULT_CONFIG_PATH="${XDG_CONFIG_HOME}/kerbin/"
else
    DEFAULT_CONFIG_PATH="${HOME}/.config/kerbin/"
fi

select_version() {
    print_step "Setting up repository for version selection"
    local current_dir=$(pwd)

    if [ ! -d "${KERBIN_BUILD_DIR}" ]; then
        print_info "Cloning repository for the first time..."
        git clone https://github.com/EmeraldPandaTurtle/Kerbin.git "${KERBIN_BUILD_DIR}"
    fi

    cd "${KERBIN_BUILD_DIR}"
    print_info "Fetching latest tags and branches..."
    git fetch --tags origin

    VERSIONS=$(
        echo "master"
        git tag --sort=-v:refname
    )

    print_step "Select the Kerbin version to install (using fzf)"
    echo -e "${BLUE}  • ${NC}Use arrows/typing to select, then press ${GREEN}Enter${NC}."

    SELECTED_VERSION=$(echo "$VERSIONS" | uniq | fzf --prompt="Kerbin Version > " --header="Select a Tag or Branch (master):")

    if [ -z "$SELECTED_VERSION" ]; then
        print_error "No version selected. Installation cancelled."
        exit 1
    fi

    print_info "Selected version: ${SELECTED_VERSION}"

    print_step "Saving selected version"
    touch "${KERBIN_INSTALL_CONFIG}"
    if grep -q "^SELECTED_VERSION=" "${KERBIN_INSTALL_CONFIG}" 2>/dev/null; then
        sed -i.bak "s|^SELECTED_VERSION=.*|SELECTED_VERSION=\"${SELECTED_VERSION}\"|" "${KERBIN_INSTALL_CONFIG}"
    else
        echo "SELECTED_VERSION=\"${SELECTED_VERSION}\"" >> "${KERBIN_INSTALL_CONFIG}"
    fi
    rm -f "${KERBIN_INSTALL_CONFIG}.bak" 2>/dev/null
    cd "${current_dir}"
}

copy_default_config() {
    local source_config_dir="${KERBIN_BUILD_DIR}/config"
    local target_config_dir="$1"

    if [ -z "$target_config_dir" ]; then
        print_error "Configuration target directory not provided to copy_default_config."
        exit 1
    fi

    if [ -d "$target_config_dir" ]; then
        if [ "$(ls -A "$target_config_dir")" ]; then
            print_info "Configuration directory already exists and is not empty. Skipping config copy to avoid overwrite."
            return 0
        fi
    fi

    if [ ! -d "${source_config_dir}" ]; then
        print_error "Source config directory '${source_config_dir}' not found in the cloned repository."
        print_info "Please ensure the selected version contains the '/config' folder."
        return 1
    fi

    print_step "Copying default configuration to ${target_config_dir}"
    mkdir -p "${target_config_dir}"
    cp -r "${source_config_dir}/." "${target_config_dir}"

    if [ $? -eq 0 ]; then
        print_success "Default configuration copied successfully."
    else
        print_error "Failed to copy default configuration."
        exit 1
    fi
}

echo -e "${CYAN}====================================================${NC}"
echo -e "${CYAN}== Kerbin: The Space-Age Text Editor Installation ==${NC}"
echo -e "${CYAN}====================================================${NC}"

REBUILD_MODE=false
SKIP_CONFIRM=false
CLEAN_BUILD=false
UPDATE_REPO=false
CLEAR_CONFIG=false

for arg in "$@"; do
    case $arg in
        --rebuild|-r) REBUILD_MODE=true ;;
        --yes|-y) SKIP_CONFIRM=true ;;
        --clean|-c) CLEAN_BUILD=true ;;
        --update|-u) UPDATE_REPO=true ;;
        --clear-config|-x) CLEAR_CONFIG=true ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --rebuild, -r             Rebuild using saved configuration"
            echo "  --yes, -y                 Skip confirmation prompts (will prevent default config copy)"
            echo "  --clean, -c               Clean build directory before building"
            echo "  --update, -u              Pull latest changes and prompt for new version"
            echo "  --clear-config, -x        Delete existing configuration directory (requires double confirmation)"
            echo "  --help, -h                Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0"
            echo "  $0 --rebuild"
            echo "  $0 -r -u"
            echo "  $0 -r -u -y"
            echo "  $0 --clear-config"
            exit 0
            ;;
    esac
done

print_step "Checking Requirements"
check_command "cargo"
check_command "git"
check_command "fzf"
print_info "All required packages are installed"

print_step "Setting up Kerbin directory structure"
mkdir -p "${KERBIN_ROOT}"
mkdir -p "${KERBIN_BIN_DIR}"
print_success "Directory structure created at ${KERBIN_ROOT}"

if [ "$CLEAR_CONFIG" = true ]; then
    print_step "Configuration clearing requested"
    echo ""
    print_error "WARNING: This will permanently delete your Kerbin configuration at:"
    echo "  ${DEFAULT_CONFIG_PATH}"
    echo ""
    echo "This action cannot be undone."

    read -p "$(echo -e ${YELLOW}"Are you absolutely sure you want to delete it? (y/n): "${NC})" -n 1
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        print_info "Cancelled configuration clearing."
        CLEAR_CONFIG=false
    else
        read -p "$(echo -e ${YELLOW}"This is your LAST WARNING. Delete config at ${DEFAULT_CONFIG_PATH}? (y/n): "${NC})" -n 1
        echo ""
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            print_info "Cancelled configuration clearing."
            CLEAR_CONFIG=false
        fi
    fi

    if [ "$CLEAR_CONFIG" = true ]; then
        if [ -d "${DEFAULT_CONFIG_PATH}" ]; then
            rm -rf "${DEFAULT_CONFIG_PATH}"
            print_success "Configuration at ${DEFAULT_CONFIG_PATH} deleted."
        else
            print_info "No configuration directory found at ${DEFAULT_CONFIG_PATH}, skipping deletion."
        fi
    fi
fi

if [ "$REBUILD_MODE" = true ]; then
    print_step "Rebuild mode: Loading saved configuration"
    if [ ! -f "${KERBIN_INSTALL_CONFIG}" ]; then
        print_error "No saved configuration found at ${KERBIN_INSTALL_CONFIG}"
        exit 1
    fi
    source "${KERBIN_INSTALL_CONFIG}"
    print_success "Configuration loaded successfully"
    print_info "Config path: ${CONFIG_PATH}"

    if [ "$UPDATE_REPO" = true ] || [ -z "$SELECTED_VERSION" ]; then
        select_version
    else
        print_info "Saved version: ${SELECTED_VERSION}"
    fi

else
    print_step "Install mode: Requesting configuration"
    COPY_DEFAULT_CONFIG=false

    if [ -f "${KERBIN_INSTALL_CONFIG}" ]; then
        source "${KERBIN_INSTALL_CONFIG}"
        echo ""
        print_info "Found existing configuration:"
        print_info "  Config path: ${CONFIG_PATH}"
        echo ""
        read -p "$(echo -e ${YELLOW}"Use existing settings? (y/n): "${NC})" -n 1
        echo ""
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            read -p "$(echo -e ${YELLOW}"Enter the path for your config [${NC}${DEFAULT_CONFIG_PATH}${YELLOW}]: "${NC})" NEW_CONFIG_PATH
            CONFIG_PATH=${NEW_CONFIG_PATH:-${DEFAULT_CONFIG_PATH}}
            if [ ! -d "${CONFIG_PATH}" ] || [ -z "$(ls -A "${CONFIG_PATH}" 2>/dev/null)" ]; then
                COPY_DEFAULT_CONFIG=true
            fi
        fi
    else
        read -p "$(echo -e ${YELLOW}"Enter the path for your config [${NC}${DEFAULT_CONFIG_PATH}${YELLOW}]: "${NC})" NEW_CONFIG_PATH
        CONFIG_PATH=${NEW_CONFIG_PATH:-${DEFAULT_CONFIG_PATH}}
        if [ ! -d "${CONFIG_PATH}" ] || [ -z "$(ls -A "${CONFIG_PATH}" 2>/dev/null)" ]; then
            COPY_DEFAULT_CONFIG=true
        fi
    fi

    print_info "Configuration path set to: ${CONFIG_PATH}"
    select_version

    if [ "$COPY_DEFAULT_CONFIG" = true ]; then
        print_info "The configuration directory at ${CONFIG_PATH} is empty or does not exist."
        if [ "$SKIP_CONFIRM" = true ]; then
            print_info "Skipping configuration copy (--yes flag provided)."
        else
            confirm "Do you want to copy the default configuration files from Kerbin to ${CONFIG_PATH}?"
            cd "${KERBIN_BUILD_DIR}"
            git reset --hard
            git clean -fd
            if ! git checkout "${SELECTED_VERSION}"; then
                print_error "Failed to checkout version ${SELECTED_VERSION} for config copy. Aborting."
                exit 1
            fi
            copy_default_config "${CONFIG_PATH}"

            CONFIG_CARGO_TOML="${CONFIG_PATH}/Cargo.toml"
            if [ -f "${CONFIG_CARGO_TOML}" ]; then
                print_step "Rewriting relative paths in config manifest (${CONFIG_CARGO_TOML})"
                sed -i.bak "s|path = \"../|path = \"${KERBIN_BUILD_DIR}/|" "${CONFIG_CARGO_TOML}"
                rm -f "${CONFIG_CARGO_TOML}.bak" 2>/dev/null
                print_success "Internal config paths updated."
            fi
            cd "${HOME}"
        fi
    fi

    print_step "Saving configuration for future rebuilds"
    cat > "${KERBIN_INSTALL_CONFIG}" << EOF
# Kerbin Installation Configuration
# Generated: $(date)
CONFIG_PATH="${CONFIG_PATH}"
SELECTED_VERSION="${SELECTED_VERSION}"
EOF
    print_success "Configuration saved to ${KERBIN_INSTALL_CONFIG}"
fi

if [ "$SKIP_CONFIRM" = false ]; then
    confirm "Ready to start building Kerbin version ${SELECTED_VERSION}?"
else
    print_info "Skipping confirmation (--yes flag provided)"
fi

if [ "$CLEAN_BUILD" = true ]; then
    print_step "Clean build requested"
    [ -d "${KERBIN_BUILD_DIR}" ] && rm -rf "${KERBIN_BUILD_DIR}" && print_success "Build directory cleaned"
fi

if [ ! -d "${KERBIN_BUILD_DIR}" ]; then
    print_step "Cloning repository"
    git clone https://github.com/TheEmeraldBee/Kerbin.git "${KERBIN_BUILD_DIR}"
fi

cd "${KERBIN_BUILD_DIR}"
print_step "Checking out selected version: ${SELECTED_VERSION}"
if [ "$REBUILD_MODE" = true ] && [ "$UPDATE_REPO" = false ]; then
    git fetch --tags origin
fi
git reset --hard
git clean -fd
git pull --force
git checkout "${SELECTED_VERSION}"
print_success "Repository is now at version: ${SELECTED_VERSION}"

print_step "Clearing old installation"
rm -f "${KERBIN_BIN_DIR}/kerbin" 2>/dev/null || true

MAIN_CARGO_TOML="${KERBIN_BUILD_DIR}/kerbin/Cargo.toml"
print_step "Setting config path in main Cargo.toml"
sed -i.bak "s|config = { path = \"../config\" }|config = { path = \"${CONFIG_PATH}\" }|" "${MAIN_CARGO_TOML}"
rm -f "${MAIN_CARGO_TOML}.bak"

print_step "Building Editor"
cargo build --release

print_step "Moving build to install path"
cp "${KERBIN_BUILD_DIR}/target/release/kerbin" "${KERBIN_BIN_DIR}/kerbin"
chmod +x "${KERBIN_BIN_DIR}/kerbin"

INSTALL_SCRIPT_SOURCE="${KERBIN_BUILD_DIR}/install.sh"
if [ -f "${INSTALL_SCRIPT_SOURCE}" ]; then
    cp "${INSTALL_SCRIPT_SOURCE}" "${KERBIN_BIN_DIR}/kerbin-install"
    chmod +x "${KERBIN_BIN_DIR}/kerbin-install"
    print_success "Install script copied to ${KERBIN_BIN_DIR}/kerbin-install"
fi

print_step "Checking PATH configuration"
if [[ ":$PATH:" != *":${KERBIN_BIN_DIR}:"* ]]; then
    echo ""
    print_info "NOTE: ${KERBIN_BIN_DIR} is not in your PATH"
    echo -e "${YELLOW}    export PATH=\"${KERBIN_BIN_DIR}:\$PATH\"${NC}"
else
    print_success "${KERBIN_BIN_DIR} is already in your PATH"
fi

print_success "Kerbin (${SELECTED_VERSION}) built successfully!"
print_info "Root: ${KERBIN_ROOT}"
print_info "Binary: ${KERBIN_BIN_DIR}/kerbin"
print_info "Config: ${CONFIG_PATH}"
print_info "Version: ${SELECTED_VERSION}"
print_info "Build cache: ${KERBIN_BUILD_DIR}"
print_info "Disk used: $(du -sh ${KERBIN_BUILD_DIR} | cut -f1)"

