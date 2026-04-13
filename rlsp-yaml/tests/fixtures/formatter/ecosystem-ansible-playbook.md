---
test-name: ecosystem-ansible-playbook
category: ecosystem
idempotent: true
---

# Test: Ansible Playbook Round-Trip

An Ansible playbook using YAML 1.1 keywords (`yes`, `no`, `on`, `off`) as
plain scalar values. Verifies the formatter is idempotent and preserves these
as plain strings.

Ref: Ansible playbook syntax

## Test-Document

```yaml
---
- name: Configure web server
  hosts: webservers
  become: yes
  gather_facts: yes
  vars:
    app_enabled: yes
    debug_mode: no
    service_started: on
  tasks:
    - name: Install nginx
      apt:
        name: nginx
        state: present
        update_cache: yes

    - name: Start nginx
      service:
        name: nginx
        state: started
        enabled: yes
```
