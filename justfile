# Conventions: https://github.com/casey/just

import '.just/build.just'
import '.just/deploy.just'
import '.just/test.just'

image_base := env('DOCKER_IMAGE', 'litptcl/lit-node-express')
# Unique UUID tag per deploy (override with DOCKER_TAG to pin a specific build)
image_tag := env('DOCKER_TAG', `uuidgen | tr '[:upper:]' '[:lower:]' | tr -d '\n'`)
image := image_base + ':' + image_tag
app_name := env('PHALA_APP_NAME', 'lit-api-server')
instance_type := env('PHALA_INSTANCE_TYPE', 'tdx.small')

# List available recipes (default when invoked with no args)
default:
    @just --list
