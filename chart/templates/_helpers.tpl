{{/*
Expand the name of the chart.
*/}}
{{- define "presence.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Service name.
*/}}
{{- define "presence.serviceName" -}}
{{- list (default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-") "service" | join "-" }}
{{- end }}


{{/*
Short namespace.
*/}}
{{- define "presence.shortNamespace" -}}
{{- $shortns := regexSplit "-" .Release.Namespace -1 | first }}
{{- if has $shortns (list "production" "p") }}
{{- else }}
{{- $shortns }}
{{- end }}
{{- end }}

{{/*
Namespace in ingress path.
converts as follows:
- testing01 -> t01
- staging01-classroom-ng -> s01/classroom-ng
- producion-webinar-ng -> webinar-foxford
*/}}
{{- define "presence.ingressPathNamespace" -}}
{{- $ns_head := regexSplit "-" .Release.Namespace -1 | first }}
{{- $ns_tail := regexSplit "-" .Release.Namespace -1 | rest | join "-" }}
{{- if eq $ns_head "production" }}
{{- regexReplaceAll "(.*)-ng(.*)" $ns_tail "${1}-foxford${2}" }}
{{- else }}
{{- $v := list (regexReplaceAll "(.)[^\\d]*(.+)" $ns_head "${1}${2}") $ns_tail | compact | join "/" }}
{{- regexReplaceAll "(.*)-ng(.*)" $v "${1}-foxford${2}" }}
{{- end }}
{{- end }}

{{/*
Ingress path.
*/}}
{{- define "presence.ingressPath" -}}
{{- $shortns := regexSplit "-" .Release.Namespace -1 | first }}
{{- list "" (include "presence.ingressPathNamespace" .) (include "presence.name" .) | join "/" }}
{{- end }}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this (by the DNS naming spec).
If release name contains chart name it will be used as a full name.
*/}}
{{- define "presence.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "presence.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "presence.labels" -}}
helm.sh/chart: {{ include "presence.chart" . }}
app.kubernetes.io/name: {{ include "presence.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
k8s-app: {{ include "presence.name" . }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "presence.selectorLabels" -}}
app.kubernetes.io/name: {{ include "presence.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app: {{ include "presence.name" . }}
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "presence.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "presence.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Create volumeMount name from audience and secret name
*/}}
{{- define "presence.volumeMountName" -}}
{{- $audience := index . 0 -}}
{{- $secret := index . 1 -}}
{{- printf "%s-%s-secret" $audience $secret | replace "." "-" }}
{{- end }}
