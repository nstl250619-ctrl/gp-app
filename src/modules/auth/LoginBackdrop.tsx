// 登录门面背景层（§16）：深色渐变 + 网格 + 节点网络 SVG + 双光晕 + 池水 + 数据流粒子。
import { useMemo } from 'react';

// 与 CSS 修饰类 .login-screen__particle--1/--2/--3 对齐（避免 --0 无样式、蓝色粒子缺失）
const PARTICLE_VARIANTS = [1, 2, 3] as const;

// 平台节点（绿池中转连接的 AI 工具）：图标徽章 + 微光圆环 + 错落漂浮
const NODES = [
  { abbr: 'WB', name: 'WorkBuddy', x: 240, y: 180, delay: '0s', dur: '10s' },
  { abbr: 'CB', name: 'CodeBuddy', x: 940, y: 150, delay: '1.4s', dur: '11s' },
  { abbr: 'CG', name: 'CodexGPT', x: 600, y: 150, delay: '2.8s', dur: '9s' },
  { abbr: 'OC', name: 'OpenClaw', x: 1030, y: 480, delay: '4.2s', dur: '12s' },
  { abbr: 'HM', name: 'Hermes', x: 210, y: 520, delay: '0.7s', dur: '10.5s' },
  { abbr: 'CL', name: 'Claude', x: 320, y: 660, delay: '2.1s', dur: '11.5s' },
  { abbr: 'CU', name: 'Cursor', x: 880, y: 680, delay: '3.5s', dur: '9.5s' },
] as const;

export default function LoginBackdrop() {
  // 数据流粒子（一次性生成；颜色走 CSS 修饰类，避免硬编码 hex）
  const particles = useMemo(
    () =>
      Array.from({ length: 26 }, (_, i) => ({
        left: Math.random() * 100,
        top: 30 + Math.random() * 70,
        size: 2 + Math.random() * 3,
        variant: PARTICLE_VARIANTS[i % PARTICLE_VARIANTS.length],
        duration: 6 + Math.random() * 9,
        delay: Math.random() * 8,
      })),
    [],
  );

  const coreStyle = { fill: 'var(--login-node-green)' } as const;
  const labelStyle = { fill: 'rgba(255,255,255,0.38)', fontSize: 9 } as const;

  return (
    <div className="login-screen__bg">
      <div className="login-screen__grid" />
      <svg className="login-screen__net" viewBox="0 0 1200 800" preserveAspectRatio="xMidYMid slice">
        <defs>
          <linearGradient id="loginLinkGrad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" style={{ stopColor: 'var(--login-node-green)' }} />
            <stop offset="1" style={{ stopColor: 'var(--login-node-blue)' }} />
          </linearGradient>
          <linearGradient id="nodeGrad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" style={{ stopColor: 'var(--login-node-blue)' }} />
            <stop offset="1" style={{ stopColor: 'var(--login-node-green)' }} />
          </linearGradient>
        </defs>
        <path d="M600 400 L240 180" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        <path d="M600 400 L940 150" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        <path d="M600 400 L600 150" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        <path d="M600 400 L1030 480" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        <path d="M600 400 L210 520" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        <path d="M600 400 L320 660" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        <path d="M600 400 L880 680" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        {/* GreenPool core */}
        <circle cx="600" cy="400" r="46" style={coreStyle} opacity="0.16" />
        <circle cx="600" cy="400" r="16" style={coreStyle} />
        <text x="600" y="435" textAnchor="middle" style={labelStyle}>GreenPool</text>
        {/* Platform nodes: glow ring + icon badge + float */}
        {NODES.map((n) => (
          <g key={n.name} className="net-node" style={{ animationDelay: n.delay, animationDuration: n.dur }}>
            <circle cx={n.x} cy={n.y} r="26" className="node-ring" />
            <circle cx={n.x} cy={n.y} r="26" className="node-ring node-ring--spin" />
            <circle cx={n.x} cy={n.y} r="17" fill="url(#nodeGrad)" />
            <text x={n.x} y={n.y + 3.5} textAnchor="middle" className="node-abbr">{n.abbr}</text>
            <text x={n.x} y={n.y + 42} textAnchor="middle" style={labelStyle}>{n.name}</text>
          </g>
        ))}
      </svg>
      <div className="login-screen__orb login-screen__orb--green" />
      <div className="login-screen__orb login-screen__orb--blue" />
      <div className="login-screen__pool" />
      {particles.map((p, i) => (
        <span
          key={i}
          className={`login-screen__particle login-screen__particle--${p.variant}`}
          style={{
            left: `${p.left}%`,
            top: `${p.top}%`,
            width: p.size,
            height: p.size,
            animationDuration: `${p.duration}s`,
            animationDelay: `${p.delay}s`,
          }}
        />
      ))}
    </div>
  );
}
