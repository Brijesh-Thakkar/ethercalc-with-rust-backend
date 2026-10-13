# Rust Backend for rust peer experiments using SocialCalc/EtherCalc and libp2p

A modern Rust implementation of the SocialCalc (Aspiring Investments) backend using Axum framework for testing rust peer. This backend provides a high-performance, async web server with multi-database support and cloud storage integration.

## Features

- **Authentication**: JWT-based authentication with bcrypt password hashing and session management
- **File Management**: Multi-database support with user isolation and secure file operations
- **Email Integration**: AWS SES integration for email notifications and password resets
- **Cloud Storage**: Flexible storage backends supporting AWS S3 and MinIO (S3-compatible)
- **In-App Purchases**: Purchase tracking and validation system
- **RESTful API**: Clean REST API with JSON responses and comprehensive error handling
- **Web Templates**: Server-side rendering with Askama template engine
- **PDF Generation**: HTML to PDF conversion capabilities
- **Image Processing**: Image handling and manipulation endpoints

## Tech Stack

- **Framework**: Axum 0.7 (async web framework built on Tokio)
- **Databases**:
  - MongoDB 2.8 (primary document store)
  - MySQL 8.0 with SQLx (relational data)
- **Authentication**: JWT + bcrypt with cookie-based sessions
- **Cloud Services**:
  - AWS S3 (production storage)
  - MinIO (local S3-compatible storage)
  - AWS SES (email service)
- **Serialization**: Serde JSON
- **Async Runtime**: Tokio with full features
- **HTTP Client**: Reqwest
- **Validation**: Validator for input validation
- **Template Engine**: Askama for server-side rendering

## Getting Started

### Prerequisites

**Without Docker:**

- Rust 1.70 or later
- MongoDB 7.0 or later
- MySQL 8.0 or later
- AWS credentials (for S3 and SES) or MinIO for local development

**With Docker:**

- Docker 20.10 or later
- Docker Compose v2.0 or later

### Environment Variables

Create a `.env` file in the project root:

```env
ENVIRONMENT=development
PORT=8080

COOKIE_SECRET=your-cookie-secret-key-here
JWT_SECRET=your-jwt-secret-key-here
# Signs session cookies; at least 32 bytes. Required unless ENVIRONMENT is development/test.
SECRET_KEY=generate-with-openssl-rand-base64-48

STORAGE_BACKEND=minio

MONGO_URI=mongodb://localhost:27017
MONGO_DATABASE=touchcalc

MYSQL_DSN=mysql://root:password@localhost:3306/touchcalc

AWS_ACCESS_KEY_ID=your-aws-access-key
AWS_SECRET_ACCESS_KEY=your-aws-secret-key
AWS_REGION=us-east-1
S3_BUCKET=aspiring-cloud-storage

MINIO_ENDPOINT=localhost:9000
MINIO_ACCESS_KEY=minioadmin
MINIO_SECRET_KEY=minioadmin
MINIO_BUCKET=touchcalc-storage
MINIO_SSL=false

FROM_EMAIL=your-email@domain.com

TEMPLATES_PATH=./web/templates
STATIC_PATH=./web/static
```

### Setup with Docker

1. Clone the repository:

   ```bash
   git clone <repository-url>
   cd rust-backend-infra-calc
   ```

2. Start all services:

   ```bash
   docker compose up -d
   ```

3. Verify services are running:

   ```bash
   docker compose ps
   ```

4. View logs:

   ```bash
   docker compose logs -f rust-backend
   ```

5. Access the application:

   - Backend API: http://localhost:8080
   - MinIO Console: http://localhost:9001
   - MongoDB: localhost:27017
   - MySQL: localhost:3306

6. Stop services:

   ```bash
   docker compose down
   ```

7. Remove all data volumes:
   ```bash
   docker compose down -v
   ```

### Setup without Docker

1. Clone the repository:

   ```bash
   git clone <repository-url>
   cd rust-backend-infra-calc
   ```

2. Install and start MongoDB:

   ```bash
   # Windows (with Chocolatey)
   choco install mongodb
   # Start MongoDB service
   net start MongoDB
   ```

3. Install and start MySQL:

   ```bash
   # Windows (with Chocolatey)
   choco install mysql
   # Start MySQL service
   net start MySQL80
   ```

4. Install MinIO (optional, for local S3 storage):

   ```bash
   # Download MinIO server
   # Windows PowerShell
   Invoke-WebRequest -Uri "https://dl.min.io/server/minio/release/windows-amd64/minio.exe" -OutFile "minio.exe"

   # Start MinIO
   .\minio.exe server ./data --console-address ":9001"
   ```

5. Create databases:

   ```bash
   # MongoDB
   mongosh
   use touchcalc

   # MySQL
   mysql -u root -p
   CREATE DATABASE touchcalc;
   ```

6. Install Rust dependencies and build:

   ```bash
   cargo build --release
   ```

7. Run the application:
   ```bash
   cargo run --release
   ```

The server will start on `http://localhost:8080`

## API Endpoints

### Authentication

- `POST /login` - User login with email and password
- `POST /register` - User registration with validation
- `GET /logout` - User logout and session cleanup
- `POST /lostpw` - Password reset request via email
- `POST /pwreset` - Complete password reset with token

### File Operations

- `GET /save` - List all user files with metadata
- `POST /save` - Save or update file content
- `POST /insert` - Retrieve specific file content
- `POST /usersheet` - Handle user sheet operations and permissions
- `POST /restore` - Restore file from backup
- `POST /downloadfile` - Download file with authentication

### Web Application

- `GET /webapp` - Serve web application interface
- `POST /webapp` - Handle web app actions (save, delete, list files)
- `GET /allusersheets` - View all accessible user sheets
- `GET /importcollab` - Import collaborative sheets
- `POST /importcollabload` - Load imported collaborative data

### Business Operations

- `GET /business` - Business logic endpoints
- `POST /business` - Execute business operations

### Finance Operations

- `GET /finance` - Financial data endpoints
- `POST /finance` - Process financial transactions

### Utilities

- `POST /runasemailer` - Send transactional emails via SES
- `GET /runas` - Execute run-as operations
- `GET /htmltopdf` - Convert HTML to PDF
- `POST /htmltopdf` - Process HTML to PDF with custom options
- `GET /iconimg` - Retrieve application icons
- `POST /iconimg` - Upload and process images

### Cloud Storage

- `GET /amazon` - AWS S3 operations
- `POST /amazon` - Upload files to S3
- `GET /dropbox` - Dropbox integration (future)
- `POST /dropbox` - Dropbox file operations (future)

### In-App Purchases

- `GET /inapppurchases` - Retrieve purchase history
- `POST /inapppurchases` - Process and validate purchases

### Health Check

- `GET /health` - Service health status

## Database Schema

### MongoDB Collections

- `users` - User accounts, authentication credentials, and profiles
- `files` - File content, metadata, and version history
- `sessions` - Active user sessions and tokens

### MySQL Tables

- `user_sheets` - Sheet metadata and permissions
- `in_app_purchases` - Purchase records and validation
- `user_preferences` - User settings and configurations

## Storage Architecture

The application supports multiple storage backends:

- **AWS S3**: Production-grade cloud storage for files and assets
- **MinIO**: S3-compatible local storage for development and testing
- **MongoDB GridFS**: Embedded file storage for smaller deployments

Storage backend is configurable via `STORAGE_BACKEND` environment variable.

## Security Features

- **Password Security**: bcrypt hashing with cost factor 12
- **JWT Authentication**: Secure token-based authentication with expiration
- **Cookie-based Sessions**: HTTP-only, secure cookies for session management
- **User Data Isolation**: Row-level security ensuring users can only access their own data
- **Input Validation**: Comprehensive validation using the validator crate
- **SQL Injection Protection**: Parameterized queries with SQLx
- **CORS Configuration**: Configurable cross-origin resource sharing
- **Rate Limiting**: Request throttling to prevent abuse

## Error Handling

The API returns consistent JSON responses for all endpoints:

**Success Response:**

```json
{
  "result": "ok",
  "data": {
    "key": "value"
  }
}
```

**Error Response:**

```json
{
  "result": "fail",
  "message": "Detailed error message",
  "error_code": "ERROR_CODE"
}
```

HTTP status codes are used appropriately:

- `200` - Success
- `400` - Bad Request
- `401` - Unauthorized
- `403` - Forbidden
- `404` - Not Found
- `500` - Internal Server Error

## Development

### Run in Development Mode

```bash
cargo run
```

### Run Tests

```bash
cargo test
```

### Run Tests with Output

```bash
cargo test -- --nocapture
```

### Watch Mode (requires cargo-watch)

```bash
cargo install cargo-watch
cargo watch -x run
```

### Format Code

```bash
cargo fmt
```

### Lint Code

```bash
cargo clippy
```

### Check Without Building

```bash
cargo check
```

## Project Structure

```
rust-backend-infra-calc/
├── src/
│   ├── main.rs              # Application entry point
│   ├── config.rs            # Configuration management
│   ├── auth.rs              # Authentication logic
│   ├── db.rs                # Database connections
│   ├── models.rs            # Data models
│   ├── session.rs           # Session management
│   ├── middleware.rs        # HTTP middleware
│   ├── utils.rs             # Utility functions
│   ├── handlers/            # Request handlers
│   │   ├── auth.rs
│   │   ├── app.rs
│   │   ├── webapp.rs
│   │   └── ...
│   ├── services/            # Business logic
│   │   ├── email.rs
│   │   ├── storage.rs
│   │   └── mod.rs
│   └── routes/              # Route definitions
├── web/
│   ├── templates/           # HTML templates (Askama)
│   └── static/              # CSS, JS, images
├── tests/                   # Integration tests
├── Cargo.toml               # Rust dependencies
├── Dockerfile               # Container image definition
├── docker-compose.yml       # Multi-container orchestration
└── .env                     # Environment variables (not in git)
```
