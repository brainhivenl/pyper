# pyper (PHP + Hyper = <3)

## Why?

I'm not a fan of PHP but as a service provider in the cloud-native space we have to support PHP applications on our managed Kubernetes environments.

As a company building modern infrastructure, we work with a diverse range of clients, some of whom rely on PHP for their applications. Our goal is to provide efficient and streamlined solutions for all our customers, regardless of their technology stack.

Traditionally, to dockerize PHP applications, we've had to package PHP-FPM with Nginx, resulting in a more complex setup. However, given that we already have an existing gateway (Envoy) in our infrastructure, we wanted to explore a more lightweight alternative.

## Enter pyper

Pyper is our attempt to create a minimal HTTP server that supports FastCGI, specifically tailored for our use case. By leveraging this lightweight solution, we aim to:

1. Simplify the deployment process for PHP applications
1. Reduce resource overhead by eliminating the need for a full-fledged web server like Nginx
1. Seamlessly integrate with our existing Envoy gateway
1. Provide a more Kubernetes-friendly solution for running PHP applications

This approach allows us to maintain a modern, efficient infrastructure while still supporting clients who rely on PHP. Pyper serves as a bridge between legacy PHP applications and our cutting-edge cloud-native environment.

## Features

- Lightweight HTTP server (based on Hyper) with FastCGI support
- Optimized for Kubernetes deployments
- Minimal resource footprint
- Fastcgi keep-alive support
- Easy (opiniated) integration with existing PHP-FPM setups

## Project status

Pyper is currently under active development and is not yet production-ready.
We are working on adding more features and improving stability before releasing the first version.

## License

Pyper is licensed under the MIT license. See the [LICENSE](LICENSE.md) file for more details.
The FastCGI client library used in this project is licensed under the Apache license.

## Open-source

- [fastcgi-client](https://github.com/jmjoy/fastcgi-client-rs)
- [hyper](https://github.com/hyperium/hyper)
- [bb8](https://github.com/djc/bb8)
