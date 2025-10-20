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
      echo "  --rebuild, -r    Rebuild using saved configuration"
      echo "  --yes, -y        Skip confirmation prompts"
      echo "  --clean, -c      Clean build directory before building"
      echo "  --update, -u     Pull latest changes from repository"
      echo "  --help, -h       Show this help message"
      echo ""
      echo "Examples:"
      echo "  $0               First time install (interactive)"
      echo "  $0 --rebuild     Rebuild with saved settings (with confirmation)"
      echo "  $0 -r -y         Rebuild with saved settings (no confirmation)"
      echo "  $0 -r -c         Clean rebuild (removes cargo cache)"
      echo "  $0 -r -u         Rebuild with latest changes from repository"
      echo "  $0 -r -u -y      Fast rebuild with updates (no confirmation)"
      echo ""
      echo "Installation location: ${KERBIN_ROOT}"
      echo "  Binary: ${KERBIN_BIN_DIR}/kerbin"
      echo "  Scripts: ${KERBIN_BIN_DIR}/kerbin-install"
      echo "  Build cache: ${KERBIN_BUILD_DIR}"
      exit 0
      ;;
  esac
done

print_step "Checking Versions"
check_command "cargo"
check_command "git"
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
else
  print_step "Install mode: Requesting configuration"
  
  # Check if config exists and offer to use it
  if [ -f "${KERBIN_INSTALL_CONFIG}" ]; then
    source "${KERBIN_INSTALL_CONFIG}"
    echo ""
    print_info "Found existing configuration:"
    print_info "  Config path: ${CONFIG_PATH}"
    echo ""
    read -p "$(echo -e ${YELLOW}"Use existing settings? (y/n): "${NC})" -n 1
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
      # Get new configuration
      read -p "$(echo -e ${YELLOW}"Enter the path for your config [${NC}${HOME}/.config/kerbin/${YELLOW}]: "${NC})" CONFIG_PATH
      CONFIG_PATH=${CONFIG_PATH:-${HOME}/.config/kerbin/}
    fi
  else
    # No existing config, get new input
    read -p "$(echo -e ${YELLOW}"Enter the path for your config [${NC}${HOME}/.config/kerbin/${YELLOW}]: "${NC})" CONFIG_PATH
    CONFIG_PATH=${CONFIG_PATH:-${HOME}/.config/kerbin/}
  fi
  
  # Save configuration for future rebuilds
  print_step "Saving configuration for future rebuilds"
  cat > "${KERBIN_INSTALL_CONFIG}" << EOF
# Kerbin Installation Configuration
# Generated: $(date)
CONFIG_PATH="${CONFIG_PATH}"
EOF
  print_success "Configuration saved to ${KERBIN_INSTALL_CONFIG}"
fi

if [ "$SKIP_CONFIRM" = false ]; then
  confirm "Ready to start building?"
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

# Setup or update the persistent build directory
if [ -d "${KERBIN_BUILD_DIR}" ]; then
  if [ "$UPDATE_REPO" = true ]; then
    print_step "Updating existing build directory"
    cd "${KERBIN_BUILD_DIR}"
    
    # Reset any local changes
    print_info "Resetting local changes..."
    git reset --hard HEAD
    
    # Fetch latest changes
    print_info "Fetching latest changes..."
    git fetch origin
    
    # Force pull to match remote
    print_info "Updating to latest version..."
    git reset --hard origin/main || git reset --hard origin/master
    
    print_success "Repository updated successfully"
  else
    print_step "Using existing build directory (no update requested)"
    cd "${KERBIN_BUILD_DIR}"
    
    # Show current commit info
    CURRENT_COMMIT=$(git rev-parse --short HEAD)
    CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
    print_info "Current version: ${CURRENT_BRANCH} (${CURRENT_COMMIT})"
    print_info "Use --update flag to pull latest changes"
  fi
else
  print_step "Creating persistent build directory at ${KERBIN_BUILD_DIR}"
  git clone https://github.com/TheEmeraldBee/Kerbin.git "${KERBIN_BUILD_DIR}"
  cd "${KERBIN_BUILD_DIR}"
  print_success "Repository cloned successfully"
fi

print_step "Clearing old installation"
if [ -f "${KERBIN_BIN_DIR}/kerbin" ]; then
  print_info "Removing old build"
  rm -f "${KERBIN_BIN_DIR}/kerbin"
else
  print_info "No previous build at location. Skipping."
fi

print_step "Setting config path"
sed -i.bak "s|config = { path = \"../config\" }|config = { path = \"$CONFIG_PATH\" }|" Cargo.toml
rm -f Cargo.toml.bak

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
  print_success "Successfully rebuilt Kerbin with saved settings!"
  print_info "Build directory preserved at: ${KERBIN_BUILD_DIR}"
else
  print_success "Successfully built Kerbin, now go have fun!"
  print_info "Kerbin installed to: ${KERBIN_BIN_DIR}/kerbin"
  print_info "Install script available at: ${KERBIN_BIN_DIR}/kerbin-install"
  print_info "Build directory preserved at: ${KERBIN_BUILD_DIR}"
  echo ""
  print_info "To rebuild later, run: kerbin-install --rebuild"
  print_info "To rebuild without prompts, run: kerbin-install --rebuild --yes"
  print_info "To rebuild with updates, run: kerbin-install --rebuild --update"
  print_info "To force a clean build, run: kerbin-install --rebuild --clean"
fi

echo ""
print_info "Installation summary:"
print_info "  Root directory: ${KERBIN_ROOT}"
print_info "  Binary: ${KERBIN_BIN_DIR}/kerbin"
print_info "  Scripts: ${KERBIN_BIN_DIR}/kerbin-install"
print_info "  Config: ${CONFIG_PATH}"
print_info "  Build cache: ${KERBIN_BUILD_DIR}"
print_info "  Disk space used: $(du -sh ${KERBIN_BUILD_DIR} | cut -f1)"
