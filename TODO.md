# leni-deni: P2P File Sharing Application

## Overview
leni-deni is a peer-to-peer file sharing application written in Rust. It allows users to share files directly with each other without requiring a central server.

## Project Goals
- Create a lightweight, efficient P2P file sharing tool
- Support direct file transfers between peers
- Implement peer discovery on local networks
- Provide a simple CLI interface for file operations
- Ensure secure and encrypted file transfers

## Milestones

### 1. Core Networking Infrastructure
- [x] Implement basic TCP/UDP socket handling
- [x] Create a connection manager for peer connections
- [x] Design protocol for peer communication
- [x] Implement peer discovery using mDNS/local network broadcasting

### 2. File Operations
- [x] Design file chunking mechanism for large file transfers
- [x] Implement file integrity verification (checksums/hashes)
- [x] Create file metadata structure
- [x] Set up file transfer resumption capability

### 3. User Interface
- [x] Implement CLI commands for basic operations
- [x] Add progress indicators for file transfers
- [x] Create peer listing and discovery interface
- [x] Implement file/directory sharing controls

### 4. Security Features
- [x] Add end-to-end encryption for file transfers
- [x] Implement basic authentication between peers
- [x] Add transfer verification and validation
- [x] Create permissions system for shared files

### 5. Advanced Features
- [x] Implement NAT traversal techniques
- [x] Add bandwidth management and throttling
- [x] Create distributed tracking of shared resources
- [x] Support for transfer queuing and scheduling
