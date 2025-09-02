# Security & Authentication Specification (Phase 2)

## Overview
Security architecture for the universal solar monitoring solution designed for local deployment on low-power devices like Raspberry Pi and Nucbox systems. For v1 (RS485-first), authentication is deferred; deploy in trusted local networks with the API and WebSocket on a single port. TLS and authentication will be introduced in a later phase.

## Security Model

### 1. Threat Model for Edge Deployment
```rust
// Primary threats for local solar monitoring systems
pub enum ThreatLevel {
    Low,    // Normal operation, trusted local network
    Medium, // Exposed to internet, some access controls needed
    High,   // Critical infrastructure, full security required
}

pub struct SecurityContext {
    threat_level: ThreatLevel,
    network_exposure: NetworkExposure,
    device_constraints: DeviceConstraints,
}

#[derive(Debug, Clone)]
pub enum NetworkExposure {
    LocalOnly,          // Only accessible on local network
    VpnAccess,          // Accessible via VPN
    InternetExposed,    // Directly accessible from internet
}

#[derive(Debug, Clone)]
pub struct DeviceConstraints {
    pub cpu_cores: u8,
    pub memory_mb: u32,
    pub storage_type: StorageType, // SD card, eMMC, SSD
    pub crypto_acceleration: bool,
}
```

### 2. Lightweight Authentication System
```rust
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use jwt_simple::prelude::*;
use std::collections::HashMap;

pub struct AuthenticationManager {
    /// Local user store (for edge deployment)
    users: Arc<RwLock<HashMap<String, User>>>,
    
    /// JWT signing key (generated on first run)
    jwt_key: HS256Key,
    
    /// Authentication configuration
    config: AuthConfig,
    
    /// Session store for active sessions
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub password_hash: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserRole {
    Admin,      // Full system access
    Operator,   // Read-write device access
    Viewer,     // Read-only access
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Enable authentication (false for local-only deployments)
    pub enabled: bool,
    
    /// JWT token expiration time
    pub token_expiry_hours: u32,
    
    /// Password requirements
    pub min_password_length: usize,
    
    /// Rate limiting for login attempts
    pub max_login_attempts: u32,
    pub lockout_duration_minutes: u32,
    
    /// Session management
    pub max_concurrent_sessions: u32,
}

impl AuthenticationManager {
    pub async fn new(config: AuthConfig) -> Result<Self> {
        let jwt_key = Self::load_or_generate_jwt_key().await?;
        
        let mut auth_manager = Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            jwt_key,
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        };
        
        // Create default admin user if no users exist
        auth_manager.ensure_default_user().await?;
        
        Ok(auth_manager)
    }

    async fn load_or_generate_jwt_key() -> Result<HS256Key> {
        let key_path = "./config/jwt_key";
        
        if let Ok(key_data) = tokio::fs::read(key_path).await {
            HS256Key::from_bytes(&key_data)
        } else {
            // Generate new key for first-time setup
            let key = HS256Key::generate();
            
            // Save key for persistence across restarts
            if let Ok(parent) = std::path::Path::new(key_path).parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(key_path, key.to_bytes()).await?;
            
            Ok(key)
        }
    }

    pub async fn authenticate(&self, username: &str, password: &str) -> Result<AuthResult> {
        if !self.config.enabled {
            // Skip authentication for local-only deployments
            return Ok(AuthResult::Success {
                token: self.create_anonymous_token()?,
                user_role: UserRole::Admin,
            });
        }

        let users = self.users.read().await;
        
        match users.get(username) {
            Some(user) if user.enabled => {
                // Verify password using Argon2
                let argon2 = Argon2::default();
                let password_hash = PasswordHash::new(&user.password_hash)?;
                
                match argon2.verify_password(password.as_bytes(), &password_hash) {
                    Ok(_) => {
                        drop(users);
                        
                        // Update last login
                        self.update_last_login(username).await?;
                        
                        // Create JWT token
                        let token = self.create_user_token(user).await?;
                        
                        Ok(AuthResult::Success {
                            token,
                            user_role: user.role.clone(),
                        })
                    }
                    Err(_) => Ok(AuthResult::InvalidCredentials),
                }
            }
            Some(_) => Ok(AuthResult::AccountDisabled),
            None => Ok(AuthResult::InvalidCredentials),
        }
    }

    async fn create_user_token(&self, user: &User) -> Result<String> {
        let claims = Claims::with_custom_claims(
            UserClaims {
                username: user.username.clone(),
                role: user.role.clone(),
            },
            Duration::from_hours(self.config.token_expiry_hours as u64),
        );
        
        self.jwt_key.authenticate(claims).map_err(Into::into)
    }

    pub async fn verify_token(&self, token: &str) -> Result<UserClaims> {
        let claims = self.jwt_key.verify_token::<UserClaims>(token, None)?;
        Ok(claims.custom)
    }

    pub async fn create_user(&self, username: String, password: String, role: UserRole) -> Result<()> {
        if password.len() < self.config.min_password_length {
            return Err(AuthError::WeakPassword);
        }

        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = argon2.hash_password(password.as_bytes(), &salt)?
            .to_string();

        let user = User {
            username: username.clone(),
            password_hash,
            role,
            created_at: Utc::now(),
            last_login: None,
            enabled: true,
        };

        self.users.write().await.insert(username, user);
        self.save_users().await?;
        
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserClaims {
    pub username: String,
    pub role: UserRole,
}

pub enum AuthResult {
    Success { token: String, user_role: UserRole },
    InvalidCredentials,
    AccountDisabled,
    TooManyAttempts,
}
```

### 3. TLS Configuration for Low-Power Devices
```rust
use rustls::{ServerConfig, Certificate, PrivateKey};
use tokio_rustls::TlsAcceptor;

pub struct TlsManager {
    config: TlsConfig,
    acceptor: Option<TlsAcceptor>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    /// Enable TLS (disable for local development)
    pub enabled: bool,
    
    /// Certificate paths (use self-signed for local deployment)
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    
    /// Auto-generate self-signed cert if paths not provided
    pub auto_generate_cert: bool,
    
    /// TLS versions to support (optimize for low-power)
    pub min_tls_version: TlsVersion,
    
    /// Cipher suites optimized for ARM processors
    pub prefer_chacha_poly: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub enum TlsVersion {
    TLS12,
    TLS13,
}

impl TlsManager {
    pub async fn new(config: TlsConfig) -> Result<Self> {
        let acceptor = if config.enabled {
            Some(Self::create_tls_acceptor(&config).await?)
        } else {
            None
        };
        
        Ok(Self { config, acceptor })
    }

    async fn create_tls_acceptor(config: &TlsConfig) -> Result<TlsAcceptor> {
        let (cert_chain, private_key) = if config.auto_generate_cert 
            && (config.cert_path.is_none() || config.key_path.is_none()) {
            Self::generate_self_signed_cert().await?
        } else {
            Self::load_cert_and_key(config).await?
        };

        let mut server_config = ServerConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)?;

        // Optimize for low-power ARM devices
        if config.prefer_chacha_poly {
            // ChaCha20-Poly1305 is faster on ARM without AES acceleration
            server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        }

        Ok(TlsAcceptor::from(Arc::new(server_config)))
    }

    async fn generate_self_signed_cert() -> Result<(Vec<Certificate>, PrivateKey)> {
        use rcgen::{Certificate as RcgenCert, CertificateParams, DistinguishedName};
        
        let mut params = CertificateParams::new(vec!["localhost".to_string()]);
        params.distinguished_name = DistinguishedName::new();
        params.distinguished_name.push(rcgen::DnType::CommonName, "Solar Monitor");
        params.distinguished_name.push(rcgen::DnType::OrganizationName, "Local Solar Monitoring");
        
        let cert = RcgenCert::from_params(params)?;
        let cert_der = cert.serialize_der()?;
        let private_key_der = cert.serialize_private_key_der();
        
        Ok((
            vec![Certificate(cert_der)],
            PrivateKey(private_key_der),
        ))
    }
}
```

### 4. API Security Middleware
```rust
use axum::{
    extract::Request,
    http::{StatusCode, HeaderMap},
    middleware::Next,
    response::Response,
};

pub struct SecurityMiddleware {
    auth_manager: Arc<AuthenticationManager>,
    rate_limiter: Arc<RateLimiter>,
    config: SecurityConfig,
}

#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// CORS configuration for web interface
    pub cors_origins: Vec<String>,
    
    /// API rate limiting (requests per minute)
    pub api_rate_limit: u32,
    
    /// Enable request logging for security auditing
    pub audit_requests: bool,
    
    /// Paths that don't require authentication
    pub public_paths: Vec<String>,
}

impl SecurityMiddleware {
    pub async fn auth_middleware(
        auth_manager: Arc<AuthenticationManager>,
        mut req: Request,
        next: Next,
    ) -> Result<Response, StatusCode> {
        let path = req.uri().path();
        
        // Check if path is public
        if Self::is_public_path(path) {
            return Ok(next.run(req).await);
        }

        // Extract authorization header
        let auth_header = req.headers()
            .get("authorization")
            .and_then(|header| header.to_str().ok())
            .and_then(|auth| auth.strip_prefix("Bearer "));

        match auth_header {
            Some(token) => {
                match auth_manager.verify_token(token).await {
                    Ok(claims) => {
                        // Add user claims to request extensions
                        req.extensions_mut().insert(claims);
                        Ok(next.run(req).await)
                    }
                    Err(_) => Err(StatusCode::UNAUTHORIZED),
                }
            }
            None => Err(StatusCode::UNAUTHORIZED),
        }
    }

    pub async fn rate_limit_middleware(
        rate_limiter: Arc<RateLimiter>,
        req: Request,
        next: Next,
    ) -> Result<Response, StatusCode> {
        let client_ip = Self::extract_client_ip(&req);
        
        if rate_limiter.check_rate_limit(&client_ip).await {
            Ok(next.run(req).await)
        } else {
            Err(StatusCode::TOO_MANY_REQUESTS)
        }
    }

    fn extract_client_ip(req: &Request) -> String {
        // Try various headers for client IP (reverse proxy aware)
        req.headers()
            .get("x-forwarded-for")
            .or_else(|| req.headers().get("x-real-ip"))
            .and_then(|header| header.to_str().ok())
            .and_then(|ip| ip.split(',').next())
            .map(|ip| ip.trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }
}

// Simple in-memory rate limiter suitable for edge devices
pub struct RateLimiter {
    requests: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
    limit: u32,
    window: Duration,
}

impl RateLimiter {
    pub fn new(requests_per_minute: u32) -> Self {
        Self {
            requests: Arc::new(RwLock::new(HashMap::new())),
            limit: requests_per_minute,
            window: Duration::from_secs(60),
        }
    }

    pub async fn check_rate_limit(&self, client_id: &str) -> bool {
        let mut requests = self.requests.write().await;
        let now = Instant::now();
        
        let client_requests = requests.entry(client_id.to_string())
            .or_insert_with(Vec::new);
        
        // Remove old requests outside the time window
        client_requests.retain(|&time| now.duration_since(time) < self.window);
        
        // Check if under limit
        if client_requests.len() < self.limit as usize {
            client_requests.push(now);
            true
        } else {
            false
        }
    }
}
```

### 5. Device Security for Edge Deployment
```rust
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

pub struct DeviceSecurity {
    config: DeviceSecurityConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceSecurityConfig {
    /// Restrict file permissions
    pub secure_file_permissions: bool,
    
    /// Run with minimal privileges
    pub drop_privileges: bool,
    
    /// Enable system hardening
    pub system_hardening: bool,
    
    /// Firewall configuration
    pub firewall_rules: Vec<FirewallRule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FirewallRule {
    pub port: u16,
    pub protocol: String, // "tcp" or "udp"
    pub allow_from: Vec<String>, // IP ranges or "local"
}

impl DeviceSecurity {
    pub async fn apply_security_hardening(&self) -> Result<()> {
        if self.config.secure_file_permissions {
            self.secure_file_permissions().await?;
        }
        
        if self.config.drop_privileges {
            self.drop_privileges()?;
        }
        
        if self.config.system_hardening {
            self.apply_system_hardening().await?;
        }
        
        Ok(())
    }

    async fn secure_file_permissions(&self) -> Result<()> {
        let config_dirs = [
            "./config",
            "./data",
            "./logs",
        ];
        
        for dir in &config_dirs {
            if let Ok(metadata) = tokio::fs::metadata(dir).await {
                if metadata.is_dir() {
                    let permissions = Permissions::from_mode(0o750); // rwxr-x---
                    tokio::fs::set_permissions(dir, permissions).await?;
                }
            }
        }
        
        // Secure sensitive files
        let sensitive_files = [
            "./config/jwt_key",
            "./config/users.json",
            "./config/system.toml",
        ];
        
        for file in &sensitive_files {
            if tokio::fs::metadata(file).await.is_ok() {
                let permissions = Permissions::from_mode(0o600); // rw-------
                tokio::fs::set_permissions(file, permissions).await?;
            }
        }
        
        Ok(())
    }

    fn drop_privileges(&self) -> Result<()> {
        // On Unix systems, drop to non-root user if running as root
        #[cfg(unix)]
        {
            use nix::unistd::{getuid, setuid, Uid};
            
            if getuid().is_root() {
                // Try to switch to 'nobody' user or create service user
                if let Ok(nobody_uid) = Self::get_nobody_uid() {
                    setuid(nobody_uid)?;
                    tracing::info!("Dropped privileges to user ID: {}", nobody_uid);
                }
            }
        }
        
        Ok(())
    }

    #[cfg(unix)]
    fn get_nobody_uid() -> Result<Uid> {
        use nix::unistd::{User, Uid};
        
        // Try to get 'nobody' user
        if let Some(user) = User::from_name("nobody")? {
            Ok(user.uid)
        } else {
            // Fallback to a high UID if nobody doesn't exist
            Ok(Uid::from_raw(65534))
        }
    }
}
```

### 6. Configuration Security
```toml
# Security configuration optimized for edge devices
[security]
enabled = true
threat_level = "medium" # low, medium, high

[security.authentication]
enabled = true
token_expiry_hours = 24
min_password_length = 8
max_login_attempts = 5
lockout_duration_minutes = 15

[security.tls]
enabled = true
auto_generate_cert = true
min_tls_version = "TLS12"
prefer_chacha_poly = true # Better for ARM without AES acceleration

[security.api]
cors_origins = ["https://localhost:3000"] # Frontend dev server
api_rate_limit = 100 # requests per minute
audit_requests = true

# Public endpoints that don't require auth
public_paths = [
    "/health",
    "/metrics", 
    "/api/v1/status"
]

[security.device]
secure_file_permissions = true
drop_privileges = true
system_hardening = true

# Simple firewall rules for local deployment
[[security.firewall_rules]]
port = 8080
protocol = "tcp"
allow_from = ["192.168.0.0/16", "10.0.0.0/8", "127.0.0.1"]

## WebSocket served on same port as API (8080)
```

### 7. Lightweight Audit Logging
```rust
pub struct SecurityAuditor {
    log_file: Arc<Mutex<tokio::fs::File>>,
    config: AuditConfig,
}

#[derive(Debug, Clone)]
pub struct AuditConfig {
    pub enabled: bool,
    pub log_successful_auth: bool,
    pub log_failed_auth: bool,
    pub log_api_requests: bool,
    pub max_log_size_mb: u64,
    pub log_rotation_count: u32,
}

#[derive(Debug, Serialize)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub user: Option<String>,
    pub client_ip: String,
    pub details: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub enum AuditEventType {
    LoginSuccess,
    LoginFailed,
    ApiRequest,
    ConfigChange,
    SecurityViolation,
}

impl SecurityAuditor {
    pub async fn log_event(&self, event: AuditEvent) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let json_line = serde_json::to_string(&event)?;
        let mut file = self.log_file.lock().await;
        file.write_all(format!("{}\n", json_line).as_bytes()).await?;
        file.flush().await?;

        // Check if log rotation is needed
        let metadata = file.metadata().await?;
        if metadata.len() > self.config.max_log_size_mb * 1024 * 1024 {
            self.rotate_logs().await?;
        }

        Ok(())
    }
}
```

## Edge Device Optimizations

### Resource-Aware Security
- **Lightweight crypto**: ChaCha20-Poly1305 for ARM without AES acceleration
- **Minimal memory footprint**: In-memory session store with size limits
- **Efficient rate limiting**: Simple time-window based implementation
- **Self-signed certificates**: Automatic generation for local deployment

### Deployment Considerations
- **Local-first security**: Authentication optional for local-only access
- **Minimal attack surface**: Only essential ports open
- **Simple user management**: Local file-based user store
- **Resource monitoring**: Security overhead tracking for low-power devices

This security specification provides practical protection suitable for edge deployment while maintaining minimal resource usage on devices like Raspberry Pi and similar low-power systems.
