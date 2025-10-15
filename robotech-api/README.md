# RoboTech-api

API definitions and data transfer objects for the RoboTech platform.

## Overview

This crate provides the API interface definitions and data transfer objects (DTOs) for the RoboTech platform. It includes:

- Unified response objects (RO) for consistent API responses
- Response result types and codes
- Data structures used across the platform

## Modules

### RO (Response Object)

The RO module defines a unified response format used throughout the API:

- `Ro<E>`: Main response structure with generic extra data
- `RoResult`: Response result enumeration (Success, IllegalArgument, Warn, Fail)
- `RoCode`: Constants for specific business error codes

## Usage

This crate is intended to be used as a dependency in the RoboTech platform services to ensure consistent API responses and data structures.

## License

This project is licensed under the MIT License - see the [LICENSE](../LICENSE) file for details.