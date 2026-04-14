---
test-name: ecosystem-ansible-handlers
category: ecosystem
idempotent: true
---

# Test: Ansible Handlers Round-Trip

An Ansible playbook with handlers, `when` conditions, `loop`, and `notify`.
Verifies the formatter is idempotent across common Ansible task and handler
patterns.

Ref: Ansible playbook syntax — handlers, loop, when, notify

## Test-Document

```yaml
---
- name: Configure application
  hosts: app_servers
  become: true
  vars:
    app_name: myapp
    app_user: deploy
    app_dir: /opt/myapp

  tasks:
    - name: Create application directory
      ansible.builtin.file:
        path: "{{ app_dir }}"
        state: directory
        owner: "{{ app_user }}"
        mode: "0755"

    - name: Install dependencies
      ansible.builtin.apt:
        name:
          - curl
          - git
          - build-essential
        state: present
        update_cache: true
      when: ansible_os_family == "Debian"

    - name: Deploy configuration files
      ansible.builtin.template:
        src: "{{ item.src }}"
        dest: "{{ item.dest }}"
        owner: "{{ app_user }}"
        mode: "0644"
      loop:
        - src: app.conf.j2
          dest: "{{ app_dir }}/app.conf"
        - src: logging.conf.j2
          dest: "{{ app_dir }}/logging.conf"
      notify: Restart application

  handlers:
    - name: Restart application
      ansible.builtin.systemd:
        name: "{{ app_name }}"
        state: restarted
        enabled: true
```
