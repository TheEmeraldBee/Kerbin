#!/usr/bin/env bash
# ======================================
# Install/Rebuild script for Kerbin
#
# Automatically requests install paths and builds the editor
# Can rebuild using saved settings with --rebuild flag
# Uses persistent build directory for faster rebuilds
# Everything centralized in ~/.kerbin/
# ======================================
# Exit if a command exits with non-zero status.
set -e

# --- Styles ---
BLUE='\033[0;34m'
CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# --- Helpers ---
print_step() {
    echo -e "\n${CYAN}>>> $1${NC}"
}

print_info() {
    echo -e "${BLUE}  • $1${NC}"
}

print_success() {
    echo -e "${GREEN}  ✔ $1${NC}"
}

print_error() {
    echo -e "${RED}  ✖ ERROR: $1${NC}"
}

confirm() {
    echo ""
    read -p "$(echo -e ${YELLOW}"$1 (y/n): "${NC})" -n 1
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        print_info "Cancelled"
        exit 1
    fi
}

# Function to check if a command is available in the system's PATH
check_command() {
    if ! command -v "$1" &>/dev/null; then
        print_error "Command '$1' could not be found. Please install it to continue."
        exit 1
    fi
    print_success "$1 is installed."
}

# --- Centralized Kerbin Directory ---
KERBIN_ROOT="${HOME}/.kerbin"
KERBIN_INSTALL_CONFIG="${KERBIN_ROOT}/install_config"
KERBIN_BIN_DIR="${KERBIN_ROOT}/bin"
KERBIN_BUILD_DIR="${KERBIN_ROOT}/build"

# --- Variables ---
SELECTED_VERSION=""
CONFIG_PATH="" # Initialize CONFIG_PATH here

# Function to get user to select a version (tag or branch) using fzf
select_version() {
    print_step "Setting up repository for version selection"

    # Save current directory to return to
    local current_dir=$(pwd)

    # Clone or ensure we are inside the build directory
    if [ ! -d "${KERBIN_BUILD_DIR}" ]; then
        print_info "Cloning repository for the first time..."
        git clone https://github.com/TheEmeraldBee/Kerbin.git "${KERBIN_BUILD_DIR}"
    fi

    # Navigate to build directory for git operations
    cd "${KERBIN_BUILD_DIR}"

    print_info "Fetching latest tags and branches..."
    git fetch --tags origin

    # Get 'master', and all tags, prioritizing tags by version number (latest first)
    # Using 'git tag --sort=-v:refname' ensures tags are sorted by version
    VERSIONS=$(
        echo "master"
        git tag --sort=-v:refname
    )

    print_step "Select the Kerbin version to install (using fzf)"
    echo -e "${BLUE}  • ${NC}Use arrows/typing to select, then press ${GREEN}Enter${NC}."

    # Use fzf to select the version
    SELECTED_VERSION=$(echo "$VERSIONS" | uniq | fzf --prompt="Kerbin Version > " --header="Select a Tag or Branch (master):")

    if [ -z "$SELECTED_VERSION" ]; then
        print_error "No version selected. Installation cancelled."
        exit 1
    fi

    print_info "Selected version: ${SELECTED_VERSION}"

    # Save selected version for rebuilds
    print_step "Saving selected version"
    # Ensure config file exists before trying to sed
    touch "${KERBIN_INSTALL_CONFIG}"
    # Use sed to safely replace or add SELECTED_VERSION
    if grep -q "^SELECTED_VERSION=" "${KERBIN_INSTALL_CONFIG}" 2>/dev/null; then
        sed -i.bak "s|^SELECTED_VERSION=.*|SELECTED_VERSION=\"${SELECTED_VERSION}\"|" "${KERBIN_INSTALL_CONFIG}"
    else
        echo "SELECTED_VERSION=\"${SELECTED_VERSION}\"" >> "${KERBIN_INSTALL_CONFIG}"
    fi
    rm -f "${KERBIN_INSTALL_CONFIG}.bak" 2>/dev/null
    
    # Return to the original directory 
    cd "${current_dir}" 
}

# NEW FUNCTION: Copies the default config directory from the repository
copy_default_config() {
    local source_config_dir="${KERBIN_BUILD_DIR}/config"
    local target_config_dir="$1"

    if [ -z "$target_config_dir" ]; then
        print_error "Configuration target directory not provided to copy_default_config."
        exit 1
    fi

    if [ -d "$target_config_dir" ]; then
        # Only warn if the dir is not empty
        if [ "$(ls -A "$target_config_dir")" ]; then
            print_info "Configuration directory already exists and is not empty. Skipping config copy to avoid overwrite."
            return 0
        fi
    fi

    # Check if the source config directory exists in the cloned repository
    if [ ! -d "${source_config_dir}" ]; then
        print_error "Source config directory '${source_config_dir}' not found in the cloned repository."
        print_info "Please ensure the selected version contains the '/config' folder."
        return 1
    fi

    print_step "Copying default configuration to ${target_config_dir}"

    mkdir -p "${target_config_dir}"
    # Copy contents of source_config_dir/* to target_config_dir
    cp -r "${source_config_dir}/." "${target_config_dir}"

    if [ $? -eq 0 ]; then
        print_success "Default configuration copied successfully."
    else
        print_error "Failed to copy default configuration."
        exit 1
    fi
}

# --- Main Script Logic ---
echo -e "${CYAN}====================================================${NC}"
echo -e "${CYAN}== Kerbin: The Space-Age Text Editor Installation ==${NC}"
echo -e "${CYAN}====================================================${NC}"

# Parse flags
REBUILD_MODE=false
SKIP_CONFIRM=false
CLEAN_BUILD=false
UPDATE_REPO=false

for arg in "$@"; do
    case $arg in
        --rebuild|-r)
            REBUILD_MODE=true
            ;;
        --yes|-y)
            SKIP_CONFIRM=true
            ;;
        --clean|-c)
            CLEAN_BUILD=true
            ;;
        --update|-u)
            UPDATE_REPO=true
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --rebuild, -r           Rebuild using saved configuration"
            echo "  --yes, -y               Skip confirmation prompts (will prevent default config copy)"
            echo "  --clean, -c             Clean build directory before building"
            echo "  --update, -u            Pull latest changes and prompt for new version"
            echo "  --help, -h              Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0                  First time install (interactive)"
            echo "  $0 --rebuild          Rebuild with saved settings (with confirmation)"
            echo "  $0 -r -u              Rebuild with latest changes and select a new version"
            echo "  $0 -r -u -y             Fast rebuild with updates and new version selection (no config copy prompt)"
            echo ""
            echo "Installation location: ${KERBIN_ROOT}"
            echo "  Binary: ${KERBIN_BIN_DIR}/kerbin"
            echo "  Scripts: ${KERBIN_BIN_DIR}/kerbin-install"
            echo "  Build cache: ${KERBIN_BUILD_DIR}"
            exit 0
            ;;
    esac
done

print_step "Checking Requirements"
check_command "cargo"
check_command "git"
check_command "fzf"
print_info "All required packages are installed"

# Create kerbin root directory structure
print_step "Setting up Kerbin directory structure"
mkdir -p "${KERBIN_ROOT}"
mkdir -p "${KERBIN_BIN_DIR}"
print_success "Directory structure created at ${KERBIN_ROOT}"

# Determine if we're using saved config or getting new input
if [ "$REBUILD_MODE" = true ]; then
    print_step "Rebuild mode: Loading saved configuration"
    
    if [ ! -f "${KERBIN_INSTALL_CONFIG}" ]; then
        print_error "No saved configuration found at ${KERBIN_INSTALL_CONFIG}"
        print_info "Run without --rebuild flag to perform initial installation."
        exit 1
    fi
    
    # Load saved configuration
    source "${KERBIN_INSTALL_CONFIG}"
    
    print_success "Configuration loaded successfully"
    print_info "Config path: ${CONFIG_PATH}"
    
    if [ "$UPDATE_REPO" = true ] || [ -z "$SELECTED_VERSION" ]; then
        if [ "$UPDATE_REPO" = true ]; then
            print_info "Update requested (--update flag), selecting a new version..."
        elif [ -z "$SELECTED_VERSION" ]; then
            print_error "Saved configuration is missing a version. Forcing new selection."
        fi
        select_version # Calls the new selection function
    else
        print_info "Saved version: ${SELECTED_VERSION}"
    fi

else
    print_step "Install mode: Requesting configuration"
    
    # --- Configuration Path Logic ---
    DEFAULT_CONFIG_PATH="${HOME}/.config/kerbin/"
    COPY_DEFAULT_CONFIG=false

    if [ -f "${KERBIN_INSTALL_CONFIG}" ]; then
        # Found existing config, offer to use it
        source "${KERBIN_INSTALL_CONFIG}"
        echo ""
        print_info "Found existing configuration:"
        print_info "  Config path: ${CONFIG_PATH}"
        echo ""
        read -p "$(echo -e ${YELLOW}"Use existing settings? (y/n): "${NC})" -n 1
        echo ""
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            # Get new configuration path
            read -p "$(echo -e ${YELLOW}"Enter the path for your config [${NC}${DEFAULT_CONFIG_PATH}${YELLOW}]: "${NC})" NEW_CONFIG_PATH
            CONFIG_PATH=${NEW_CONFIG_PATH:-${DEFAULT_CONFIG_PATH}}
            
            # Check if the new path (default or user-provided) exists and is not empty
            if [ ! -d "${CONFIG_PATH}" ] || [ -z "$(ls -A "${CONFIG_PATH}" 2>/dev/null)" ]; then
                COPY_DEFAULT_CONFIG=true
            fi
        fi
    else
        # NO EXISTING CONFIG: Prompt the user for the config path
        read -p "$(echo -e ${YELLOW}"Enter the path for your config [${NC}${DEFAULT_CONFIG_PATH}${YELLOW}]: "${NC})" NEW_CONFIG_PATH
        CONFIG_PATH=${NEW_CONFIG_PATH:-${DEFAULT_CONFIG_PATH}}
        
        # Check if the chosen path exists and is not empty
        if [ ! -d "${CONFIG_PATH}" ] || [ -z "$(ls -A "${CONFIG_PATH}" 2>/dev/null)" ]; then
            COPY_DEFAULT_CONFIG=true
        fi
    fi
    
    print_info "Configuration path set to: ${CONFIG_PATH}"

    # Force version selection on initial install or if config was changed
    select_version

    if [ "$COPY_DEFAULT_CONFIG" = true ]; then
        print_info "The configuration directory at ${CONFIG_PATH} is empty or does not exist."

        if [ "$SKIP_CONFIRM" = true ]; then
            # Do not copy config files if -y or --yes is provided.
            print_info "Skipping configuration copy because --yes/-y flag was provided."
            print_info "You must manually configure files at ${CONFIG_PATH}."
        else
            # Only prompt and copy if confirmation is NOT skipped.
            confirm "Do you want to copy the default configuration files from Kerbin to ${CONFIG_PATH}?"
            
            # Ensure we are in the build directory and have the correct version checked out 
            # to guarantee the 'config' folder in the repo is the correct one.
            print_step "Preparing repository to copy config for version: ${SELECTED_VERSION}"
            
            # The select_version function already ensured the repo is cloned. Now check it out.
            cd "${KERBIN_BUILD_DIR}"
            git reset --hard
            git clean -fd
            
            # Use 'git checkout' to switch to the correct version for copying the config
            if ! git checkout "${SELECTED_VERSION}"; then
                print_error "Failed to checkout version ${SELECTED_VERSION} for config copy. Aborting."
                exit 1
            fi

            # Now copy the config files
            copy_default_config "${CONFIG_PATH}"
            
            # --- NEW PATH REWRITING FOR CONFIGURATION MANIFEST ---
            CONFIG_CARGO_TOML="${CONFIG_PATH}/Cargo.toml"
            
            if [ -f "${CONFIG_CARGO_TOML}" ]; then
                print_step "Rewriting relative paths in config manifest (${CONFIG_CARGO_TOML})"
                
                # Use a single sed command to replace all instances of `../` with the absolute build path.
                # The '|' delimiter is used for sed to avoid conflicts with the '/' in the paths.
                # The 'g' flag ensures all instances on a line are replaced.
                # We use a leading space in the search pattern " ../" to avoid replacing relative paths that might be part of other URLs or filenames that aren't path dependencies.
                sed -i.bak "s|path = \"../|path = \"${KERBIN_BUILD_DIR}/|" "${CONFIG_CARGO_TOML}"
                rm -f "${CONFIG_CARGO_TOML}.bak" 2>/dev/null
                
                print_success "Internal config paths updated to absolute build paths."
            else
                print_info "No Cargo.toml found in ${CONFIG_PATH}, skipping internal path rewriting for config."
            fi
            # --- END NEW PATH REWRITING ---

            # Change back to script's working directory before proceeding 
            cd "${HOME}"
        fi
    fi

    # Save configuration for future rebuilds (ensure SELECTED_VERSION is saved)
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

# Handle persistent build directory
if [ "$CLEAN_BUILD" = true ]; then
    print_step "Clean build requested: Removing build directory"
    if [ -d "${KERBIN_BUILD_DIR}" ]; then
        rm -rf "${KERBIN_BUILD_DIR}"
        print_success "Build directory cleaned"
    else
        print_info "No build directory to clean"
    fi
fi

# Setup build directory and checkout version (redundant cloning check, but safe)
if [ ! -d "${KERBIN_BUILD_DIR}" ]; then
    print_step "Cloning repository to ${KERBIN_BUILD_DIR}"
    git clone https://github.com/TheEmeraldBee/Kerbin.git "${KERBIN_BUILD_DIR}"
fi

# IMPORTANT: Navigate to the build directory for all compilation steps
cd "${KERBIN_BUILD_DIR}"
print_step "Checking out selected version: ${SELECTED_VERSION}"

# Fetch/Update only if we didn't do it in select_version/install-mode (i.e. REBUILD_MODE=true, UPDATE_REPO=false)
if [ "$REBUILD_MODE" = true ] && [ "$UPDATE_REPO" = false ]; then
    print_info "Fetching tags for safe checkout..."
    git fetch --tags origin
fi

# Reset and checkout the chosen tag/branch
print_info "Checking out ${SELECTED_VERSION}..."
git reset --hard
git clean -fd
if ! git checkout "${SELECTED_VERSION}"; then
    print_error "Failed to checkout version ${SELECTED_VERSION}. The version may be invalid."
    exit 1
fi
print_success "Repository is now at version: ${SELECTED_VERSION}"


print_step "Clearing old installation"
if [ -f "${KERBIN_BIN_DIR}/kerbin" ]; then
    print_info "Removing old build"
    rm -f "${KERBIN_BIN_DIR}/kerbin"
else
    print_info "No previous build at location. Skipping."
fi

# Assuming the main Cargo.toml is at the root of the build directory:
MAIN_CARGO_TOML="${KERBIN_BUILD_DIR}/kerbin/Cargo.toml"

print_step "Setting config path in main Cargo.toml"
# This is the path the *runtime* will use to find the user config files
sed -i.bak "s|config = { path = \"../config\" }|config = { path = \"${CONFIG_PATH}\" }|" "${MAIN_CARGO_TOML}"
rm -f "${MAIN_CARGO_TOML}.bak" 2>/dev/null

# --- DYNAMIC PATH REWRITING FOR COMPILATION ---
# All path dependencies for core and plugins have been removed from here, 
# as per the requirement that they only apply to the copied user config.
print_step "Checking internal path dependencies for compilation"
print_info "Only runtime config path updated in main application manifest (kerbin/Cargo.toml)."
# --- END DYNAMIC PATH REWRITING ---

print_step "Building Editor"
print_info "Using persistent build cache for faster compilation..."
cargo build --release

print_step "Moving build to install path"
cp "${KERBIN_BUILD_DIR}/target/release/kerbin" "${KERBIN_BIN_DIR}/kerbin"
chmod +x "${KERBIN_BIN_DIR}/kerbin"

print_step "Installing kerbin-install script"
# Find the install script in the build directory
INSTALL_SCRIPT_SOURCE="${KERBIN_BUILD_DIR}/install.sh"

if [ -f "${INSTALL_SCRIPT_SOURCE}" ]; then
    # Copy the install script to the bin directory
    cp "${INSTALL_SCRIPT_SOURCE}" "${KERBIN_BIN_DIR}/kerbin-install"
    chmod +x "${KERBIN_BIN_DIR}/kerbin-install"
    print_success "Install script copied to ${KERBIN_BIN_DIR}/kerbin-install"
else
    print_info "Install script not found in repo, skipping self-installation"
fi

# Check if kerbin bin directory is in PATH
print_step "Checking PATH configuration"
if [[ ":$PATH:" != *":${KERBIN_BIN_DIR}:"* ]]; then
    echo ""
    print_info "NOTE: ${KERBIN_BIN_DIR} is not in your PATH"
    print_info "Add the following line to your shell configuration:"
    echo ""
    echo -e "${YELLOW}    export PATH=\"${KERBIN_BIN_DIR}:\$PATH\"${NC}"
    echo ""
    print_info "For bash: Add to ~/.bashrc or ~/.bash_profile"
    print_info "For zsh: Add to ~/.zshrc"
    print_info "For fish: Run: fish_add_path ${KERBIN_BIN_DIR}"
    echo ""
    print_info "After adding, restart your shell or run: source ~/.bashrc (or your shell's config)"
else
    print_success "${KERBIN_BIN_DIR} is already in your PATH"
fi

if [ "$REBUILD_MODE" = true ]; then
    print_success "Successfully rebuilt Kerbin (${SELECTED_VERSION}) with saved settings!"
    print_info "Build directory preserved at: ${KERBIN_BUILD_DIR}"
else
    print_success "Successfully built Kerbin (${SELECTED_VERSION}), now go have fun!"
    print_info "Kerbin installed to: ${KERBIN_BIN_DIR}/kerbin"
    print_info "Install script available at: ${KERBIN_BIN_DIR}/kerbin-install"
    print_info "Build directory preserved at: ${KERBIN_BUILD_DIR}"
    echo ""
    print_info "To rebuild later, run: kerbin-install --rebuild"
    print_info "To rebuild with updates and select a new version, run: kerbin-install --rebuild --update"
    print_info "To force a clean build, run: kerbin-install --rebuild --clean"
fi

echo ""
print_info "Installation summary:"
print_info "  Root directory: ${KERBIN_ROOT}"
print_info "  Binary: ${KERBIN_BIN_DIR}/kerbin"
print_info "  Scripts: ${KERBIN_BIN_DIR}/kerbin-install"
print_info "  Config: ${CONFIG_PATH}"
print_info "  Version: ${SELECTED_VERSION}"
print_info "  Build cache: ${KERBIN_BUILD_DIR}"
print_info "  Disk space used: $(du -sh ${KERBIN_BUILD_DIR} | cut -f1)"
