Name:           portail
Version:        0.1.0
Release:        1%{?dist}
Summary:        Unified proxy/gateway: AI Gateway + MCP Gateway + CDN cache

License:        MIT
URL:            https://github.com/peterlodri-sec/portail
Source0:        %{url}/archive/refs/tags/v%{version}.tar.gz

BuildRequires:  rust >= 1.85
BuildRequires:  cargo

%description
Portail is a unified proxy and gateway for AI services, MCP tools,
and CDN caching. Built in Rust with zero-copy I/O, SIMD-optimized
hashing, and a live TUI dashboard.

%prep
%autosetup -n %{name}-%{version}

%build
cargo build --release --locked

%check
cargo test --locked

%install
mkdir -p %{buildroot}%{_bindir}
install -m 755 target/release/%{name} %{buildroot}%{_bindir}/%{name}

%files
%license LICENSE
%doc README.md
%{_bindir}/%{name}

%changelog
* Thu Jun 26 2026 Peter Lodri <peterlodri-sec@github.com> - 0.1.0-1
- Initial release
